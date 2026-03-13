use std::{
    io::{self, Read, Write},
    panic,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use client::ClientGenerator;
use semantic::SemanticAnalysis;
use tracing_subscriber::FmtSubscriber;

use ast::{
    CloesceAst, WranglerSpec,
    err::{GeneratorErrorKind, Result},
};
use migrations::{MigrationsDilemma, MigrationsIntent};
use workers::WorkersGenerator;
use wrangler::{WranglerDefault, WranglerGenerator};

#[derive(Parser)]
#[command(name = "generate", version = "0.0.3")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Generate {
        pre_cidl_path: PathBuf,
        cidl_path: PathBuf,
        wrangler_path: PathBuf,
        workers_path: PathBuf,
        client_path: PathBuf,
        workers_domain: String,
    },
    Migrations {
        cidl_path: PathBuf,

        #[arg(long, conflicts_with = "all", required_unless_present = "all")]
        binding: Option<String>,

        #[arg(long, conflicts_with = "binding")]
        all: bool,

        #[arg(long)]
        fixed: bool,

        name: String,
        wrangler_path: PathBuf,
        root_path: PathBuf,
    },
}

fn main() {
    let subscriber = FmtSubscriber::builder().finish();

    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");

    match panic::catch_unwind(run_cli) {
        Ok(Ok(())) => std::process::exit(0),
        Ok(Err(e)) if matches!(e.kind, GeneratorErrorKind::InvalidInputFile) => {
            eprintln!(
                "==== CLOESCE ERROR ====\nAn error occurred when reading a file: {}\n",
                e.context
            );
        }
        Ok(Err(e)) => {
            eprintln!(
                r#"
==== CLOESCE ERROR ====
Error [{:?}]: {}
Phase: {:?}
Context: {}
Suggested fix: {}

"#,
                e.kind, e.description, e.phase, e.context, e.suggestion
            );
        }
        Err(e) => {
            tracing::error!("==== GENERATOR PANIC CAUGHT ====");
            let msg = e
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
                .unwrap_or("Panic occurred but couldn't extract info.");
            tracing::error!("Panic info: {}", msg);
        }
    }

    std::process::exit(1);
}

fn run_cli() -> Result<()> {
    match Cli::parse().command {
        Commands::Migrations {
            cidl_path,
            binding,
            all,
            fixed,
            name,
            wrangler_path,
            root_path,
        } => {
            generate_migrations(
                &cidl_path,
                binding.as_deref(),
                all,
                &name,
                fixed,
                &wrangler_path,
                &root_path,
            )?;
        }
        Commands::Generate {
            pre_cidl_path,
            cidl_path,
            wrangler_path,
            workers_path,
            client_path,
            workers_domain,
        } => {
            // Parsing
            let wrangler = WranglerGenerator::from_path(&wrangler_path);
            let mut spec = wrangler.as_spec();
            let mut ast = CloesceAst::from_json(&pre_cidl_path)?;

            // Analysis
            WranglerDefault::set_defaults(&mut spec, &ast);
            SemanticAnalysis::analyze(&mut ast, &spec)?;

            // Code Generation
            generate_wrangler(&wrangler_path, wrangler, spec)?;
            generate_workers(&mut ast, &workers_path)?;
            generate_client(&ast, &client_path, &workers_domain)?;

            ast.set_merkle_hash();
            write_cidl(ast, &cidl_path)?;
        }
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
            } => Self::rename_or_drop(&model_name, options, "model"),
            MigrationsDilemma::RenameOrDropColumn {
                model_name,
                column_name: attribute_name,
                options,
            } => {
                let target = format!("{model_name}.{attribute_name}");
                Self::rename_or_drop(&target, options, "attribute")
            }
        }
    }
}

impl MigrationsCli {
    fn rename_or_drop(target: &str, options: &[&String], kind: &str) -> Option<usize> {
        println!("Did you intend to rename or drop {kind} \"{target}\"?");
        println!("  [r] Rename");
        println!("  [d] Drop");
        print!("> ");
        io::stdout().flush().unwrap();

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
                io::stdout().flush().unwrap();

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

fn write_cidl(ast: CloesceAst, cidl_path: &Path) -> Result<()> {
    let mut cidl_file = open_file_or_create(cidl_path)?;
    cidl_file
        .write_all(ast.to_json().as_bytes())
        .expect("file to be written");

    Ok(())
}

fn generate_migrations(
    cidl_path: &Path,
    binding: Option<&str>,
    all: bool,
    name: &str,
    fixed: bool,
    wrangler_path: &Path,
    root_path: &Path,
) -> Result<()> {
    let wrangler = WranglerGenerator::from_path(wrangler_path);
    let spec = wrangler.as_spec();

    if spec.d1_databases.is_empty() {
        // No D1 bindings, no migrations. Exit gracefully.
        return Ok(());
    }

    let bindings: Vec<String> = if all {
        spec.d1_databases
            .iter()
            .filter_map(|db| db.binding.as_deref())
            .map(|s| s.to_string())
            .collect()
    } else {
        vec![
            binding
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
                return Err(GeneratorErrorKind::UnknownBinding
                    .to_error()
                    .with_context(format!(
                        "No D1 binding '{}' found in the wrangler config",
                        current_binding
                    )));
            }
        };

        let migrations_dir = root_path.join(db.migrations_dir.as_deref().unwrap_or("migrations"));

        std::fs::create_dir_all(&migrations_dir).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;
        let mut entries: Vec<PathBuf> = std::fs::read_dir(&migrations_dir)
            .map_err(|e| {
                GeneratorErrorKind::InvalidInputFile
                    .to_error()
                    .with_context(e.to_string())
            })?
            .filter_map(|e| e.ok().map(|d| d.path()))
            .collect();

        // Last migrated CIDL file is the last .json file in the migrations dir
        entries.sort();
        let last_migrated_cidl_path = if fixed {
            None
        } else {
            entries
                .iter()
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("json"))
                .next_back()
                .cloned()
        };

        let file_stem = if fixed {
            name.to_string()
        } else {
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();

            format!("{}_{timestamp}", name)
        };

        let migrated_cidl_path = migrations_dir.join(format!("{file_stem}.json"));
        let migrated_sql_path = migrations_dir.join(format!("{file_stem}.sql"));

        tracing::info!(
            "Starting migration for binding '{}' ({})",
            current_binding,
            migrated_sql_path.display()
        );

        let mut migrated_cidl_file = open_file_or_create(&migrated_cidl_path)?;
        let mut migrated_sql_file = open_file_or_create(&migrated_sql_path)?;

        let lm_ast = last_migrated_cidl_path
            .map(|p| ast::MigrationsAst::from_json(&p))
            .transpose()?;

        // Migrate only the models with the specified D1 binding
        let mut ast = ast::MigrationsAst::from_json(cidl_path)?;
        ast.models
            .retain(|_, m| m.d1_binding == Some(current_binding.to_string()));

        let generated_sql =
            migrations::MigrationsGenerator::migrate(&ast, lm_ast.as_ref(), &MigrationsCli);

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

fn generate_wrangler(
    wrangler_path: &Path,
    mut wrangler: WranglerGenerator,
    spec: WranglerSpec,
) -> Result<()> {
    let wrangler_file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(wrangler_path)
        .map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;

    wrangler.generate(spec, wrangler_file);
    Ok(())
}

fn generate_workers(ast: &mut CloesceAst, workers_path: &Path) -> Result<()> {
    let mut file = open_file_or_create(workers_path)?;

    let workers = WorkersGenerator::generate(ast, workers_path);
    file.write_all(workers.as_bytes())
        .expect("Could not write to file");
    Ok(())
}

fn generate_client(ast: &CloesceAst, client_path: &Path, domain: &str) -> Result<()> {
    let mut file = open_file_or_create(client_path)?;
    file.write_all(ClientGenerator::generate(ast, domain.to_string()).as_bytes())
        .expect("Could not write to file");
    Ok(())
}

fn open_file_or_create(path: &Path) -> Result<std::fs::File> {
    if path.exists() {
        std::fs::File::open(path).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;
    }
    std::fs::File::create(path).map_err(|e| {
        GeneratorErrorKind::InvalidInputFile
            .to_error()
            .with_context(e.to_string())
    })
}

pub fn read_stdin_line() -> io::Result<String> {
    let mut buf = [0u8; 1];
    let mut out = String::new();

    loop {
        match io::stdin().read(&mut buf) {
            Ok(0) => break, // EOF
            Ok(_) => {
                let c = buf[0] as char;
                out.push(c);
                if c == '\n' {
                    break;
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Ok(out)
}
