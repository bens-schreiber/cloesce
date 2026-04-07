use std::{io::Write, panic, path::PathBuf};

use clap::{Args, Parser, Subcommand};
use cli::open_file_or_create;
use frontend::{
    fmt::DisplayError,
    lexer::{CloesceLexer, LexTarget},
};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "cloesce")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    Compile(CompileArgs),
    Migrate(MigrateArgs),
}

#[derive(Args)]
#[command(name = "compile")]
pub struct CompileArgs {
    pub cloesce_dir: PathBuf,
    pub wrangler_path: PathBuf,
    pub default_migrations_path: PathBuf,
    pub worker_url: String,

    #[arg(required = true, num_args = 1.., value_name = "PATH")]
    pub targets: Vec<PathBuf>,

    // For the Cloesce regression tests. Prefixes files with "out.".
    #[arg(long)]
    pub snap: bool,
}

#[derive(Args)]
#[command(name = "migrate", version = "0.0.3")]
pub struct MigrateArgs {
    pub cidl_path: PathBuf,

    #[arg(long, conflicts_with = "all", required_unless_present = "all")]
    pub binding: Option<String>,

    #[arg(long, conflicts_with = "binding")]
    pub all: bool,

    #[arg(long)]
    pub fixed: bool,

    pub name: String,
    pub wrangler_path: PathBuf,
    pub root_path: PathBuf,
}

fn main() {
    let subscriber = FmtSubscriber::builder().finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");

    let cli = Cli::parse();
    let run = || match cli.command {
        Command::Compile(args) => compile::compile(args),
        Command::Migrate(args) => migrate::migrate(args),
    };

    match panic::catch_unwind(run) {
        Ok(Ok(())) => std::process::exit(0),
        Ok(Err(e)) => {
            tracing::error!("An error occurred: {e}");
            std::process::exit(1);
        }
        Err(e) => {
            tracing::error!("An uncaught error occurred: {:?}", e);
            std::process::abort();
        }
    }
}

mod compile {
    use codegen::{
        backend::BackendGenerator, client::ClientGenerator, wrangler::WranglerDefault,
        wrangler::WranglerGenerator,
    };
    use frontend::parser::CloesceParser;
    use semantic::SemanticAnalysis;

    use super::*;

    pub fn compile(args: CompileArgs) -> Result<(), String> {
        // Lexing
        let sources = args
            .targets
            .into_iter()
            .map(|p| {
                let src = std::fs::read_to_string(&p)
                    .unwrap_or_else(|_| panic!("Failed to read source file: {}", p.display()));
                (src, p)
            })
            .collect::<Vec<(String, PathBuf)>>();

        let lexed = CloesceLexer::lex(sources.iter().map(|(src, path)| LexTarget {
            src: src.as_str(),
            path: path.clone(),
        }));
        if lexed.has_errors() {
            lexed.display_error(&lexed.file_table);
            return Err("lexing failed".to_string());
        }

        // Parsing
        let parse = CloesceParser::parse(&lexed.results, &lexed.file_table);
        if parse.has_errors() {
            parse.display_error(&lexed.file_table);
            return Err("parsing failed".to_string());
        }

        // Semantic
        let (ast, errors) = SemanticAnalysis::analyze(&parse.ast);
        if !errors.is_empty() {
            for error in &errors {
                error.display_error(&lexed.file_table);
            }
            return Err("semantic analysis failed".to_string());
        }

        // Codegen
        let wrangler = {
            let mut generator = WranglerGenerator::from_path(&args.wrangler_path);
            let mut spec = generator.as_spec();

            WranglerDefault::set_defaults(
                &mut spec,
                &ast,
                args.default_migrations_path.to_str().unwrap(),
            );
            generator.generate(spec)
        };

        let backend = BackendGenerator::generate(&ast, &args.worker_url);
        let client = ClientGenerator::generate(&ast, &args.worker_url);

        let output_name = |name: &str| {
            if args.snap {
                format!("out.{}", name)
            } else {
                name.to_string()
            }
        };

        // Output CIDL
        {
            let cidl_path = args.cloesce_dir.join(output_name("cidl.json"));
            let mut file =
                open_file_or_create(&cidl_path).expect("Failed to create cidl output file");
            file.write_all(ast.to_json().as_bytes())
                .expect("file to be written");
        };

        // Output Wrangler
        {
            let mut wrangler_file = open_file_or_create(&args.wrangler_path)
                .expect("Failed to create wrangler output file");
            wrangler_file
                .write_all(wrangler.as_bytes())
                .expect("file to be written");
        }

        // Output backend
        {
            let backend_path = args.cloesce_dir.join(output_name("backend.ts")); // TODO: hardcoded to ts
            let mut file =
                open_file_or_create(&backend_path).expect("Failed to create backend output file");
            file.write_all(backend.as_bytes())
                .expect("file to be written");
        }

        // Output client
        {
            let client_path = args.cloesce_dir.join(output_name("client.ts")); // TODO: hardcoded to ts
            let mut file =
                open_file_or_create(&client_path).expect("Failed to create client output file");
            file.write_all(client.as_bytes())
                .expect("file to be written");
        }

        Ok(())
    }
}

mod migrate {
    use std::io::Read;

    use ast::MigrationsAst;
    use codegen::wrangler::WranglerGenerator;
    use migrations::{MigrationsDilemma, MigrationsGenerator, MigrationsIntent};

    use super::*;

    pub fn migrate(args: MigrateArgs) -> Result<(), String> {
        let wrangler = WranglerGenerator::from_path(&args.wrangler_path);
        let spec = wrangler.as_spec();

        if spec.d1_databases.is_empty() {
            // No D1 bindings, no migrations. Exit gracefully.
            tracing::warn!("No D1 bindings found in the wrangler config. Nothing to migrate.");
            return Ok(());
        }

        let bindings: Vec<String> = if args.all {
            spec.d1_databases
                .iter()
                .filter_map(|db| db.binding.as_deref())
                .map(|s| s.to_string())
                .collect()
        } else {
            vec![
                args.binding
                    .expect("clap should enforce --binding or --all")
                    .to_string(),
            ]
        };

        for current_binding in bindings {
            // Binding should exist in the wrangler config and be of type D1
            let db = spec
                .d1_databases
                .iter()
                .find(|db| db.binding.as_deref() == Some(current_binding.as_str()));

            let db = match db {
                Some(db) => db,
                None => {
                    return Err(format!(
                        "No D1 database binding named '{}' found in the wrangler config.",
                        current_binding
                    ));
                }
            };

            let migrations_dir = args
                .root_path
                .join(db.migrations_dir.as_deref().unwrap_or("migrations"));

            std::fs::create_dir_all(&migrations_dir)
                .expect("Failed to create migrations directory");
            let mut entries: Vec<PathBuf> = std::fs::read_dir(&migrations_dir)
                .expect("Failed to read migrations directory")
                .filter_map(|e| e.ok().map(|d| d.path()))
                .collect();

            // Last migrated CIDL file is the last .json file in the migrations dir
            entries.sort();
            let last_migrated_cidl_path = if args.fixed {
                None
            } else {
                entries
                    .iter()
                    .rfind(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .map(|ext| ext.eq_ignore_ascii_case("json"))
                            .unwrap_or(false)
                    })
                    .cloned()
            };

            let file_stem = if args.fixed {
                args.name.to_string()
            } else {
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("Time went backwards")
                    .as_secs();

                format!("{}_{timestamp}", args.name)
            };

            let migrated_cidl_path = migrations_dir.join(format!("{file_stem}.json"));
            let migrated_sql_path = migrations_dir.join(format!("{file_stem}.sql"));

            tracing::info!(
                "Starting migration for binding '{}' ({})",
                current_binding,
                migrated_sql_path.display()
            );

            let mut migrated_cidl_file = open_file_or_create(&migrated_cidl_path)
                .expect("Failed to create migrated CIDL file");
            let mut migrated_sql_file = open_file_or_create(&migrated_sql_path)
                .expect("Failed to create migrated SQL file");

            let lm_ast_contents = last_migrated_cidl_path
                .map(|p| std::fs::read_to_string(&p).map_err(|e| e.to_string()))
                .transpose()?;
            let lm_ast = lm_ast_contents
                .as_deref()
                .map(MigrationsAst::from_json)
                .transpose()?;

            // Migrate only the models with the specified D1 binding
            let ast_contents =
                std::fs::read_to_string(&args.cidl_path).map_err(|e| e.to_string())?;
            let mut ast = MigrationsAst::from_json(&ast_contents)?;
            ast.models
                .retain(|_, m| m.d1_binding == Some(current_binding.to_string()));

            let generated_sql = MigrationsGenerator::migrate(&ast, lm_ast.as_ref(), &MigrationsCli);

            migrated_cidl_file
                .write_all(ast.to_json().as_bytes())
                .expect("Could not write to file");
            migrated_sql_file
                .write_all(generated_sql.as_bytes())
                .expect("Could not write to file");

            tracing::info!("Finished migration for binding '{}'.", current_binding);
        }

        Ok(())
    }

    struct MigrationsCli;
    impl MigrationsIntent for MigrationsCli {
        fn ask(&self, dilemma: MigrationsDilemma) -> Option<usize> {
            match dilemma {
                MigrationsDilemma::RenameOrDropModel {
                    model_name,
                    options,
                } => Self::rename_or_drop(model_name, options, "model"),
                MigrationsDilemma::RenameOrDropColumn {
                    model_name,
                    column_name: attribute_name,
                    options,
                } => {
                    let target = format!("{model_name}.{attribute_name}");
                    let options = options.iter().map(|s| s.as_ref()).collect::<Vec<_>>();
                    Self::rename_or_drop(&target, options.as_slice(), "column")
                }
            }
        }
    }

    impl MigrationsCli {
        fn rename_or_drop(target: &str, options: &[&str], kind: &str) -> Option<usize> {
            println!("Did you intend to rename or drop {kind} \"{target}\"?");
            println!("  [r] Rename");
            println!("  [d] Drop");
            print!("> ");
            std::io::stdout().flush().unwrap();

            let line = match read_stdin_line() {
                Ok(line) => line,
                Err(_) => {
                    eprintln!("Error reading input. Aborting migrations.");
                    std::process::abort();
                }
            };

            match line.trim().to_lowercase().as_str() {
                "d" | "drop" => {
                    println!("Dropping {target}");
                    None
                }
                "r" | "rename" => {
                    println!("Select a {kind} to rename \"{target}\" to:");
                    for (i, opt) in options.iter().enumerate() {
                        println!("  [{i}] {opt}");
                    }
                    print!("> ");
                    std::io::stdout().flush().unwrap();

                    let input = match read_stdin_line() {
                        Ok(line) => line,
                        Err(_) => {
                            eprintln!("Error reading input. Aborting migrations.");
                            std::process::abort();
                        }
                    };

                    let idx = input.trim().parse::<usize>().unwrap_or_else(|_| {
                        tracing::error!("Invalid selection. Aborting migrations.");
                        std::process::abort();
                    });

                    if idx >= options.len() {
                        tracing::error!("Index out of range. Aborting migrations.");
                        std::process::abort();
                    }

                    Some(idx)
                }
                _ => {
                    tracing::error!("Invalid option. Aborting migrations.");
                    std::process::abort();
                }
            }
        }
    }

    // Necessary for reading NodeJS stdin
    pub fn read_stdin_line() -> std::io::Result<String> {
        let mut buf = [0u8; 1];
        let mut out = String::new();

        loop {
            match std::io::stdin().read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(_) => {
                    let c = buf[0] as char;
                    out.push(c);
                    if c == '\n' {
                        break;
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        Ok(out)
    }
}
