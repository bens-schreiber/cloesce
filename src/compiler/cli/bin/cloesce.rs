use std::{io::Write, panic, path::PathBuf};

use clap::{Args, Parser, Subcommand};
use cli::open_file_or_create;
use frontend::{
    fmt::DisplayError,
    lexer::{CloesceLexer, LexTarget},
};
use serde::Deserialize;
use tracing_subscriber::FmtSubscriber;

#[derive(Debug, Deserialize)]
#[serde(default)]
struct ParsedCloesceConfig {
    src_paths: Vec<String>,
    out_path: String,
    workers_url: String,
    migrations_path: String,
    #[serde(default)]
    wrangler_config_format: WranglerConfigFormat,
}

impl Default for ParsedCloesceConfig {
    fn default() -> Self {
        ParsedCloesceConfig {
            src_paths: vec![],
            out_path: ".cloesce".to_string(),
            workers_url: "http://localhost:8787".to_string(),
            migrations_path: "./migrations".to_string(),
            wrangler_config_format: WranglerConfigFormat::default(),
        }
    }
}

struct CloesceConfig {
    parsed: ParsedCloesceConfig,
    root: std::path::PathBuf,
    env: Option<String>,
}

impl CloesceConfig {
    fn cloesce_dir(&self) -> std::path::PathBuf {
        self.root.join(&self.parsed.out_path)
    }

    fn wrangler_path(&self) -> std::path::PathBuf {
        self.root
            .join(self.parsed.wrangler_config_format.wrangler_file_name())
    }

    fn cidl_path(&self) -> std::path::PathBuf {
        self.cloesce_dir().join("cidl.json")
    }

    fn load(root: &std::path::Path, env: Option<String>) -> Result<CloesceConfig, String> {
        let config_path = if let Some(env) = env.as_ref() {
            root.join(format!("cloesce.config.{}.jsonc", env))
        } else {
            root.join("cloesce.config.jsonc")
        };

        let raw = match std::fs::read_to_string(&config_path) {
            Ok(contents) => contents,
            Err(e) if matches!(e.kind(), std::io::ErrorKind::NotFound) => {
                tracing::warn!(
                    "No cloesce config found at {}. Using defaults.",
                    config_path.display()
                );
                "{}".to_string()
            }
            Err(e) => {
                return Err(format!(
                    "Failed to read cloesce config at {}: {}",
                    config_path.display(),
                    e
                ));
            }
        };

        let stripped = json_comments::StripComments::new(raw.as_bytes());
        let parsed = serde_json::from_reader(stripped)
            .map_err(|e| format!("Failed to parse {}: {}", config_path.display(), e))?;
        Ok(CloesceConfig {
            parsed,
            root: root.to_path_buf(),
            env,
        })
    }

    fn collect_source(&self, root: &std::path::Path) -> Vec<PathBuf> {
        fn is_source(path: &std::path::Path) -> bool {
            matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("cloesce") | Some("clo")
            )
        }

        let mut results = Vec::new();
        for p in &self.parsed.src_paths {
            let full = if std::path::Path::new(p).is_absolute() {
                PathBuf::from(p)
            } else {
                root.join(p)
            };

            if !full.exists() {
                tracing::warn!("src path does not exist: {}", full.display());
                continue;
            }

            if full.is_file() {
                if is_source(&full) {
                    results.push(full);
                }
                continue;
            }

            let mut queue = std::collections::VecDeque::new();
            queue.push_back(full);
            while let Some(dir) = queue.pop_front() {
                let Ok(entries) = std::fs::read_dir(&dir) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        queue.push_back(path);
                    } else if is_source(&path) {
                        results.push(path);
                    }
                }
            }
        }
        results
    }
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
enum WranglerConfigFormat {
    #[default]
    Toml,
    Jsonc,
}

impl WranglerConfigFormat {
    fn wrangler_file_name(&self) -> &'static str {
        match self {
            WranglerConfigFormat::Toml => "wrangler.toml",
            WranglerConfigFormat::Jsonc => "wrangler.jsonc",
        }
    }
}

#[derive(Parser)]
#[command(name = "cloesce")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,

    // Determine which environment to compile for.
    #[arg(long)]
    pub env: Option<String>,
}

#[derive(Subcommand)]
pub enum Command {
    Compile(CompileArgs),
    Migrate(MigrateArgs),
    Version,
}

#[derive(Args)]
#[command(name = "compile")]
pub struct CompileArgs {
    // For the Cloesce regression tests. Prefixes files with "out.".
    #[arg(long)]
    pub snap: bool,
}

#[derive(Args)]
#[command(name = "migrate")]
pub struct MigrateArgs {
    #[arg(long, conflicts_with = "all", required_unless_present = "all")]
    pub binding: Option<String>,

    #[arg(long, conflicts_with = "binding")]
    pub all: bool,

    #[arg(long)]
    pub fixed: bool,

    pub name: String,

    /// Override the CIDL input path (defaults to config-derived path)
    #[arg(long)]
    pub cidl: Option<PathBuf>,

    /// Override the wrangler config path (defaults to config-derived path)
    #[arg(long)]
    pub wrangler: Option<PathBuf>,
}

fn fetch_latest_version() -> Option<String> {
    let response = ureq::get("https://api.github.com/repos/bens-schreiber/cloesce/releases/latest")
        .header("User-Agent", "cloesce-cli")
        .call()
        .ok()?;
    let json: serde_json::Value = response.into_body().read_json().ok()?;
    let tag = json["tag_name"].as_str()?;
    Some(tag.trim_start_matches('v').to_string())
}

fn main() {
    let subscriber = FmtSubscriber::builder().finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");

    // Spawn a seperate thread so we don't impede compilation
    let update_check = std::thread::spawn(fetch_latest_version);

    let cli = Cli::parse();
    let run = || {
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let config = CloesceConfig::load(&root, cli.env)?;

        match cli.command {
            Command::Compile(args) => {
                let sources = config.collect_source(&root);
                compile::compile(args, config, sources)
            }
            Command::Migrate(args) => migrate::migrate(args, config),
            Command::Version => {
                println!("cloesce {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
        }
    };

    let result = panic::catch_unwind(run);

    let current = env!("CARGO_PKG_VERSION");
    match update_check.join().ok().flatten() {
        Some(latest) if latest != current => {
            println!("A new version of cloesce is available: v{latest} (current: v{current})");
            println!("To update, run: curl -fsSL https://cloesce.pages.dev/install.sh | sh");
        }
        Some(_) => {
            // Current version is up to date, no need to print anything
        }
        None => println!("cloesce v{current}"),
    }

    match result {
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

    pub fn compile(
        args: CompileArgs,
        config: CloesceConfig,
        target_paths: Vec<PathBuf>,
    ) -> Result<(), String> {
        if target_paths.is_empty() {
            return Err("No .clo / .cloesce source files found".to_string());
        }

        // Lexing
        let sources = target_paths
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
            let mut generator = WranglerGenerator::from_path(&config.wrangler_path());
            let env = config.env.as_deref();
            let mut spec = generator.as_spec(env);

            WranglerDefault::set_defaults(&mut spec, &ast, &config.parsed.migrations_path);
            generator.generate(spec, env)
        };

        let backend = BackendGenerator::generate(&ast, &config.parsed.workers_url);
        let client = ClientGenerator::generate(&ast, &config.parsed.workers_url);

        let output_name = |name: &str| {
            if args.snap {
                format!("out.{}", name)
            } else {
                name.to_string()
            }
        };

        // Output CIDL
        {
            let cidl_path = config.cloesce_dir().join(output_name("cidl.json"));
            let mut file =
                open_file_or_create(&cidl_path).expect("Failed to create cidl output file");
            file.write_all(ast.to_json().as_bytes())
                .expect("file to be written");
        };

        // Output Wrangler
        {
            let wrangler_path = if args.snap {
                config.cloesce_dir().join(output_name(
                    config.parsed.wrangler_config_format.wrangler_file_name(),
                ))
            } else {
                config.wrangler_path()
            };
            let mut wrangler_file =
                open_file_or_create(&wrangler_path).expect("Failed to create wrangler output file");
            wrangler_file
                .write_all(wrangler.as_bytes())
                .expect("file to be written");
        }

        // Output backend
        {
            let backend_path = config.cloesce_dir().join(output_name("backend.ts")); // TODO: hardcoded to ts
            let mut file =
                open_file_or_create(&backend_path).expect("Failed to create backend output file");
            file.write_all(backend.as_bytes())
                .expect("file to be written");
        }

        // Output client
        {
            let client_path = config.cloesce_dir().join(output_name("client.ts")); // TODO: hardcoded to ts
            let mut file =
                open_file_or_create(&client_path).expect("Failed to create client output file");
            file.write_all(client.as_bytes())
                .expect("file to be written");
        }

        Ok(())
    }
}

mod migrate {
    use ast::MigrationsAst;
    use codegen::wrangler::WranglerGenerator;
    use migrations::{MigrationsDilemma, MigrationsGenerator, MigrationsIntent};

    use super::*;

    pub fn migrate(args: MigrateArgs, config: CloesceConfig) -> Result<(), String> {
        let wrangler_path = args.wrangler.unwrap_or_else(|| config.wrangler_path());
        let cidl_path = args.cidl.unwrap_or_else(|| config.cidl_path());
        let wrangler = WranglerGenerator::from_path(&wrangler_path);
        let spec = wrangler.as_spec(config.env.as_deref());

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

            let migrations_dir = config
                .root
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
            let ast_contents = std::fs::read_to_string(&cidl_path).map_err(|e| e.to_string())?;
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

            let mut line = String::new();
            if std::io::stdin().read_line(&mut line).is_err() {
                eprintln!("Error reading input. Aborting migrations.");
                std::process::abort();
            }

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

                    let mut input = String::new();
                    if std::io::stdin().read_line(&mut input).is_err() {
                        eprintln!("Error reading input. Aborting migrations.");
                        std::process::abort();
                    }

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
}
