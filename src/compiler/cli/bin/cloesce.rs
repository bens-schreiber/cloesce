//! The Cloesce CLI, providing commands for compiling, migrating and formatting Cloesce source files.
//!
//! # Features
//!
//! The `cloesce` binary provides the following subcommands:
//!
//! - `compile`: Compiles `.clo` and `.cloesce` source files into a JSON CIDL file, a Wrangler config file,
//!   and TypeScript client and backend code. By default, the output files are placed in the `.cloesce` directory,
//!   but this can be configured in the `cloesce.jsonc` config file.
//!
//! - `migrate`: Generates a SQL migration file and a CIDL file containing only the migrated models based on the
//!   differences between the current CIDL and the last migrated CIDL.
//!
//! - `fmt`: Formats `.clo` and `.cloesce` source files according to a consistent style.
//!
//! - `version`: Displays the current version of the `cloesce` binary and checks for updates.
//!
//! # Configuration File
//!
//! The `cloesce` binary looks for a `cloesce.jsonc` configuration file ([ParsedCloesceConfig]) in the current working directory by default,
//! or `<env>.cloesce.jsonc` if the `--env` flag is provided, which specifies various settings for the compilation and migration processes.

use std::{
    collections::VecDeque,
    fs::File,
    io::Write,
    panic,
    path::{Path, PathBuf},
};

use frontend::{err::DisplayError, lexer::LexTarget};

use clap::{Args, Parser, Subcommand};
use serde::Deserialize;
use tracing_subscriber::FmtSubscriber;

/// Direct values from the <env>.closce.jsonc file
#[derive(Debug, Deserialize)]
#[serde(default)]
struct ParsedCloesceConfig {
    src_paths: Vec<String>,
    out_path: String,
    workers_url: String,
    migrations_path: String,
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
    root: PathBuf,
    env: Option<String>,
}

impl CloesceConfig {
    /// The directory of the generated output files.
    /// Defaults to <root>/.cloesce
    fn cloesce_dir(&self) -> PathBuf {
        self.root.join(&self.parsed.out_path)
    }

    /// The path to the wrangler config file to read from and write to.
    /// Defaults to <root>/wrangler.toml or <root>/wrangler.jsonc depending on the wrangler_config_format field in the config.
    fn wrangler_path(&self) -> PathBuf {
        self.root
            .join(self.parsed.wrangler_config_format.wrangler_file_name())
    }

    fn load(root: &Path, env: Option<String>) -> Result<CloesceConfig, String> {
        let config_path = if let Some(env) = env.as_ref() {
            root.join(format!("{}.cloesce.jsonc", env))
        } else {
            root.join("cloesce.jsonc")
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

    /// Scans the `src_paths` directories for `.clo` and `.cloesce` files,
    fn collect_sources(&self, root: &Path) -> Vec<PathBuf> {
        fn is_source(path: &Path) -> bool {
            matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("cloesce") | Some("clo")
            )
        }

        let mut results = Vec::new();
        for p in &self.parsed.src_paths {
            let full = {
                let p = Path::new(p);
                if p.is_absolute() {
                    p.to_path_buf()
                } else {
                    root.to_path_buf().join(p)
                }
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

            let mut queue = VecDeque::new();
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

        tracing::info!("Found {} source files.", results.len());
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
struct Cli {
    #[command(subcommand)]
    command: Command,

    // Determine which environment to compile for.
    #[arg(long, global = true)]
    env: Option<String>,
}

#[derive(Subcommand)]
enum Command {
    Compile(CompileArgs),
    Migrate(MigrateArgs),
    Fmt(FormatArgs),
    Version,
}

#[derive(Args)]
struct CompileArgs {
    /// Directory to compile. Defaults to the current directory.
    #[arg(default_value = ".")]
    dir: PathBuf,
}

#[derive(Args)]
struct MigrateArgs {
    #[arg(long, conflicts_with = "all", required_unless_present = "all")]
    binding: Option<String>,

    #[arg(long, conflicts_with = "binding")]
    all: bool,

    name: String,
}

fn open_file_or_create(path: &Path) -> Result<File, String> {
    let err = |e: std::io::Error| format!("Failed to open file {}: {}", path.display(), e);

    match File::create(path) {
        Ok(f) => Ok(f),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(err)?;
            }
            File::create(path).map_err(err)
        }
        Err(e) => Err(err(e)),
    }
}

#[derive(Args)]
struct FormatArgs {
    #[arg(long)]
    check: bool,
}

fn main() {
    let start_time = std::time::Instant::now();
    let subscriber = FmtSubscriber::builder().without_time().finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");

    let cli = Cli::parse();

    // Spawn a separate thread as to not impede the compiler.
    // `version` command will always force a fetch
    let update_check = if cfg!(debug_assertions) {
        None
    } else {
        let is_version_cmd = matches!(cli.command, Command::Version);
        Some(std::thread::spawn(move || {
            version::fetch_latest_version(is_version_cmd)
        }))
    };

    let run = || -> Result<(), String> {
        let root = std::env::current_dir().map_err(|e| e.to_string())?;

        match cli.command {
            Command::Compile(args) => {
                let root = if args.dir.is_absolute() {
                    args.dir
                } else {
                    root.join(args.dir)
                };
                let mut config = CloesceConfig::load(&root, cli.env)?;
                if config.parsed.src_paths.is_empty() {
                    config.parsed.src_paths = vec!["./".to_string()];
                }
                let sources = config.collect_sources(&root);
                compile::compile(config, sources)?;

                let elapsed = start_time.elapsed();
                tracing::info!("Compilation completed in {:.2?}", elapsed);
                Ok(())
            }
            Command::Migrate(args) => {
                let config = CloesceConfig::load(&root, cli.env)?;
                migrate::migrate(args, config)?;

                let elapsed = start_time.elapsed();
                tracing::info!("Migration completed in {:.2?}", elapsed);
                Ok(())
            }
            Command::Fmt(args) => {
                tracing::warn!("The format command is experimental, use with caution.");
                let config = CloesceConfig::load(&root, cli.env)?;
                let sources = config.collect_sources(&root);
                format::format(sources, args)?;

                let elapsed = start_time.elapsed();
                tracing::info!("Formatting completed in {:.2?}", elapsed);
                Ok(())
            }
            Command::Version => {
                println!("cloesce {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
        }
    };
    let result = panic::catch_unwind(run);

    let current = env!("CARGO_PKG_VERSION");
    match update_check.and_then(|h| h.join().ok()).flatten() {
        Some(latest) if latest != current => {
            println!(" ");
            println!("A new version of cloesce is available: v{latest} (current: v{current})");
            println!(" ");
            println!("To update, run:");
            println!("  curl -fsSL https://cloesce.pages.dev/install.sh | sh    # MacOS/Linux");
            println!(
                "  irm https://cloesce.pages.dev/install.ps1 | iex         # Windows PowerShell"
            );
            println!(" ");
        }
        Some(_) => {
            // Current version is up to date
        }
        None => println!("cloesce v{current}"),
    }

    match result {
        Ok(Ok(())) => std::process::exit(0),
        Ok(Err(e)) => {
            tracing::error!("{e}");
            std::process::exit(1);
        }
        Err(e) => {
            const CLOESCE_GITHUB_ISSUES: &str = "https://github.com/bens-schreiber/cloesce/pulls";

            tracing::error!(
                "An uncaught error occurred. Open an issue at {CLOESCE_GITHUB_ISSUES}: \n{:?}",
                e
            );
            std::process::abort();
        }
    }
}

mod compile {
    use codegen::{
        backend::BackendGenerator, client::ClientGenerator, wrangler::WranglerDefault,
        wrangler::WranglerGenerator,
    };
    use frontend::{lexer, parser};

    use super::*;

    pub fn compile(config: CloesceConfig, target_paths: Vec<PathBuf>) -> Result<(), String> {
        tracing::info!("Starting compilation with config: {:?}", config.parsed);
        if target_paths.is_empty() {
            return Err("No cloesce source files found".into());
        }

        // Load wrangler first to catch any errors before more expensive compilation steps.
        // If missing, create a default empty config.
        let (mut wrangler, mut wrangler_spec) = {
            let wrangler_path = config.wrangler_path();
            let wrangler_contents = match std::fs::read_to_string(&wrangler_path) {
                Ok(contents) => contents,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    tracing::warn!(
                        "No wrangler config found at {}. Creating default.",
                        wrangler_path.display()
                    );
                    let default = match config.parsed.wrangler_config_format {
                        WranglerConfigFormat::Toml => String::new(),
                        WranglerConfigFormat::Jsonc => "{}".to_string(),
                    };
                    open_file_or_create(&wrangler_path)?
                        .write_all(default.as_bytes())
                        .map_err(|e| {
                            format!(
                                "Failed to write default wrangler config at {}: {}",
                                wrangler_path.display(),
                                e
                            )
                        })?;
                    default
                }
                Err(e) => {
                    return Err(format!(
                        "Failed to read wrangler config at {}: {}",
                        wrangler_path.display(),
                        e
                    ));
                }
            };

            let generator = WranglerGenerator::from_contents(wrangler_contents, &wrangler_path)?;
            let env = config.env.as_deref();
            let spec = generator.as_spec(env).map_err(|e| {
                format!(
                    "Failed to process wrangler config {}: {}",
                    wrangler_path.display(),
                    e
                )
            })?;

            (generator, spec)
        };

        // Lexing
        let sources = target_paths
            .into_iter()
            .map(|p| {
                let src = std::fs::read_to_string(&p)
                    .map_err(|e| format!("Failed to read source file {}: {}", p.display(), e))?;

                Ok((src, p))
            })
            .collect::<Result<Vec<(String, PathBuf)>, String>>()
            .map_err(|e| {
                tracing::error!("{}", e);
                "Failed to read source files".to_string()
            })?;

        let (lex_results, file_table) = lexer::lex(sources.iter().map(|(src, path)| LexTarget {
            src: src.as_str(),
            path: path.clone(),
        }))
        .unwrap_or_else(|(errors, file_table)| {
            errors.display_error(&file_table);
            std::process::exit(1);
        });

        // Parsing
        let ast = parser::parse(&lex_results, &file_table).unwrap_or_else(|err| {
            err.display_error(&file_table);
            std::process::exit(1);
        });

        // Semantic
        let idl = match semantic::analyze(&ast) {
            Ok(idl) => idl,
            Err(errors) => {
                for error in errors {
                    error.display_error(&file_table);
                }
                return Err("semantic analysis failed".into());
            }
        };

        // Codegen
        let wrangler = {
            WranglerDefault::set_defaults(&mut wrangler_spec, &idl, &config.parsed.migrations_path);
            wrangler.generate(wrangler_spec, config.env.as_deref())
        };

        let backend = BackendGenerator::generate(&idl, &config.parsed.workers_url);
        let client = ClientGenerator::generate(&idl, &config.parsed.workers_url);

        let output_name = |name: &str| name.to_string();

        // Output CIDL
        {
            let cidl_path = config.cloesce_dir().join(output_name("cidl.json"));
            let mut file = open_file_or_create(&cidl_path)?;

            file.write_all(idl.to_json().as_bytes())
                .map_err(|e| format!("Failed to write CIDL file {}: {}", cidl_path.display(), e))?;
            tracing::info!("Generated JSON CIDL at {}", cidl_path.display());
        };

        // Output Wrangler
        {
            let out_wrangler_path = config.wrangler_path();
            let mut out_wrangler_file = open_file_or_create(&out_wrangler_path)?;

            out_wrangler_file
                .write_all(wrangler.as_bytes())
                .map_err(|e| {
                    format!(
                        "Failed to write wrangler file {}: {}",
                        out_wrangler_path.display(),
                        e
                    )
                })?;
            tracing::info!(
                "Generated wrangler config at {}",
                out_wrangler_path.display()
            );
        }

        // Output backend
        {
            let backend_path = config.cloesce_dir().join(output_name("backend.ts"));
            let mut file = open_file_or_create(&backend_path)?;

            file.write_all(backend.as_bytes()).map_err(|e| {
                format!(
                    "Failed to write backend file {}: {}",
                    backend_path.display(),
                    e
                )
            })?;
            tracing::info!("Generated backend code at {}", backend_path.display());
        }

        // Output client
        {
            let client_path = config.cloesce_dir().join(output_name("client.ts"));
            let mut file = open_file_or_create(&client_path)?;

            file.write_all(client.as_bytes()).map_err(|e| {
                format!(
                    "Failed to write client file {}: {}",
                    client_path.display(),
                    e
                )
            })?;
            tracing::info!("Generated client code at {}", client_path.display());
        }

        Ok(())
    }
}

mod migrate {
    use codegen::wrangler::DurableMigrationGenerator;
    use codegen::wrangler::WranglerGenerator;
    use idl::BackingKind;
    use migrations::{MigrationsDilemma, MigrationsGenerator, MigrationsIdl, MigrationsIntent};

    use super::*;

    pub fn migrate(args: MigrateArgs, config: CloesceConfig) -> Result<(), String> {
        let (wrangler_path, cidl_path) = (
            config.wrangler_path(),
            config.cloesce_dir().join("cidl.json"),
        );

        let spec = {
            let wrangler_contents = std::fs::read_to_string(&wrangler_path).map_err(|e| {
                format!(
                    "Failed to read wrangler config at {}: {}",
                    wrangler_path.display(),
                    e
                )
            })?;

            WranglerGenerator::from_contents(wrangler_contents, &wrangler_path)?
                .as_spec(config.env.as_deref())
                .map_err(|e| {
                    format!(
                        "Failed to process wrangler config {}: {}",
                        wrangler_path.display(),
                        e
                    )
                })?
        };

        let durable_bindings = spec
            .durable_objects
            .as_ref()
            .map(|dos| {
                dos.bindings
                    .iter()
                    .filter_map(|b| b.name.as_deref())
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if spec.d1_databases.is_empty() && durable_bindings.is_empty() {
            tracing::warn!(
                "No D1 or Durable Object bindings found in the wrangler config. Nothing to migrate."
            );
            return Ok(());
        }

        let bindings = if args.all {
            spec.d1_databases
                .iter()
                .filter_map(|db| db.binding.as_deref())
                .map(|s| s.to_string())
                .chain(durable_bindings.iter().cloned())
                .collect()
        } else {
            vec![
                args.binding
                    .expect("clap should enforce --binding or --all")
                    .to_string(),
            ]
        };

        let ast_contents = std::fs::read_to_string(&cidl_path)
            .map_err(|e| format!("Failed to read CIDL file {}: {}", cidl_path.display(), e))?;

        for current_binding in bindings {
            let d1_database = spec
                .d1_databases
                .iter()
                .find(|db| db.binding.as_deref() == Some(current_binding.as_str()));

            let kind = match d1_database {
                Some(_) => BackingKind::D1,
                None if durable_bindings.contains(&current_binding) => BackingKind::DurableObject,
                None => {
                    return Err(format!(
                        "No D1 database or Durable Object binding named '{}' found in the wrangler config.",
                        current_binding
                    ));
                }
            };

            // A D1 database declares its own migrations directory; a Durable Object's
            // migrations live under the configured migrations path, by binding name.
            let migrations_dir = match d1_database {
                Some(db) => config
                    .root
                    .join(db.migrations_dir.as_deref().unwrap_or("migrations")),
                None => config
                    .root
                    .join(&config.parsed.migrations_path)
                    .join(&current_binding),
            };
            std::fs::create_dir_all(&migrations_dir).map_err(|e| {
                format!(
                    "Failed to create migrations directory {}: {}",
                    migrations_dir.display(),
                    e
                )
            })?;

            // The last migrated CIDL file is the most recent timestamped file
            // within the migrations directory.
            let last_migrated_cidl_path: Option<PathBuf> = {
                let mut dir_entries = std::fs::read_dir(&migrations_dir)
                    .map_err(|e| {
                        format!(
                            "Failed to read migrations directory {}: {}",
                            migrations_dir.display(),
                            e
                        )
                    })?
                    .filter_map(|e| e.ok().map(|d| d.path()))
                    .collect::<Vec<_>>();
                dir_entries.sort();

                dir_entries
                    .iter()
                    .rfind(|p| {
                        p.extension()
                            .and_then(|e| e.to_str())
                            .map(|ext| ext.eq_ignore_ascii_case("json"))
                            .unwrap_or(false)
                    })
                    .cloned()
            };

            let timestamp: u64 = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("Time went backwards")
                .as_secs();

            let file_stem = format!("{timestamp}_{}", args.name);

            let lm_contents = last_migrated_cidl_path
                .map(|p: PathBuf| {
                    std::fs::read_to_string(&p).map_err(|e| {
                        format!(
                            "Failed to read last migrated CIDL file {}: {}",
                            p.display(),
                            e
                        )
                    })
                })
                .transpose()?;

            let lm_ast = lm_contents
                .as_deref()
                .map(MigrationsIdl::from_json)
                .transpose()?;

            // Migrate only the models backed by the specified binding.
            let idl = {
                let mut idl = MigrationsIdl::from_json(&ast_contents)?;
                idl.models.retain(|_, m| {
                    m.backing.as_ref().map(|b| b.binding) == Some(&current_binding)
                        && (kind == BackingKind::D1 || !m.primary_columns.is_empty())
                });

                idl
            };

            let generated_sql = MigrationsGenerator::migrate(&idl, lm_ast.as_ref(), &MigrationsCli);

            match kind {
                BackingKind::D1 => {
                    let mut migrated_cidl_file =
                        open_file_or_create(&migrations_dir.join(format!("{file_stem}.json")))?;
                    let mut migrated_sql_file =
                        open_file_or_create(&migrations_dir.join(format!("{file_stem}.sql")))?;

                    migrated_cidl_file
                        .write_all(idl.to_json().as_bytes())
                        .map_err(|e| format!("Failed to write migrated CIDL file: {e}"))?;
                    migrated_sql_file
                        .write_all(generated_sql.as_bytes())
                        .map_err(|e| format!("Failed to write migrated SQL file: {e}"))?;
                }
                BackingKind::DurableObject => {
                    // TODO: Locked to TS for now
                    let migration_ts =
                        DurableMigrationGenerator::generate(&args.name, timestamp, &generated_sql);

                    let mut migrated_cidl_file =
                        open_file_or_create(&migrations_dir.join(format!("{file_stem}.json")))?;
                    let mut migration_file =
                        open_file_or_create(&migrations_dir.join(format!("{file_stem}.ts")))?;

                    migrated_cidl_file
                        .write_all(idl.to_json().as_bytes())
                        .map_err(|e| format!("Failed to write migrated CIDL file: {e}"))?;
                    migration_file
                        .write_all(migration_ts.as_bytes())
                        .map_err(|e| format!("Failed to write migration file: {e}"))?;
                }
            }

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

mod version {
    use super::*;

    const GITHUB_RELEASE_API: &str =
        "https://api.github.com/repos/bens-schreiber/cloesce/releases/latest";

    const UPDATE_CHECK_INTERVAL_SECS: u64 = 3600; // 1 hour

    struct UpdateCache {
        path: PathBuf,

        /// line 1: Unix timestamp of last fetch (seconds)
        fetched_at: u64,

        /// line 2: ETag from GitHub response (or [String::default])
        etag: String,

        /// line 3: version string (without leading 'v')
        version: String,
    }

    impl UpdateCache {
        fn path() -> Option<PathBuf> {
            Some(dirs::cache_dir()?.join("cloesce").join("update_check"))
        }

        fn load() -> Option<Self> {
            let path = Self::path()?;
            let text = std::fs::read_to_string(&path).ok()?;
            let mut lines = text.splitn(3, '\n');
            let fetched_at = lines.next()?.trim().parse::<u64>().ok()?;
            let etag = lines.next()?.trim().to_string();
            let version = lines.next()?.trim().to_string();
            if version.is_empty() {
                return None;
            }
            Some(Self {
                path,
                fetched_at,
                etag,
                version,
            })
        }

        fn save(&self) {
            if let Some(parent) = self.path.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                tracing::error!(
                    "Failed to create cache directory {}: {}",
                    parent.display(),
                    e
                );
                return;
            }

            if let Err(e) = std::fs::write(
                &self.path,
                format!("{}\n{}\n{}", self.fetched_at, self.etag, self.version),
            ) {
                tracing::error!(
                    "Failed to write update cache file {}: {}",
                    self.path.display(),
                    e
                );
            }
        }
    }

    fn now_secs() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    }

    struct CurlResponse {
        status: u16,
        body: String,
        etag: String,
    }

    /// Returns [None] if curl fails to run, exits unsuccessfully, or its output
    /// cannot be parsed.
    fn curl(url: &str, etag: Option<&str>) -> Option<CurlResponse> {
        let header_dump =
            std::env::temp_dir().join(format!("cloesce_update_{}.headers", now_secs()));

        let mut cmd = std::process::Command::new("curl");
        cmd.arg("-sSL")
            .args(["-A", "cloesce-cli"])
            .args(["-D", &header_dump.to_string_lossy()])
            .args(["-w", "\n%{http_code}"]);

        // Attach If-None-Match when we have a cached ETag.
        if let Some(etag) = etag.filter(|e| !e.is_empty()) {
            cmd.args(["-H", &format!("If-None-Match: {etag}")]);
        }
        cmd.arg(url);

        let output = cmd.output().ok()?;

        let etag = {
            let headers = std::fs::read_to_string(&header_dump).unwrap_or_default();
            let _ = std::fs::remove_file(&header_dump);
            if !output.status.success() {
                return None;
            }

            // formatted like ETag: "some-etag-value"
            headers
                .lines()
                .filter_map(|line| line.split_once(':'))
                .filter(|(name, _)| name.trim().eq_ignore_ascii_case("etag"))
                .map(|(_, value)| value.trim().to_string())
                .next_back()
                .unwrap_or_default()
        };

        let stdout = String::from_utf8_lossy(&output.stdout);
        let (body, status) = stdout.rsplit_once('\n')?;
        let status = status.trim().parse().ok()?;

        Some(CurlResponse {
            status,
            body: body.to_string(),
            etag,
        })
    }

    /// Fetches the latest released version of cloesce, using a local cache to avoid
    /// hitting the GitHub API rate limit.
    ///
    /// Blocking call.
    ///
    /// Returns [None] if the fetch failed and there is no valid cache, otherwise returns the version string (x.x.x)
    ///
    pub fn fetch_latest_version(force: bool) -> Option<String> {
        let cached = UpdateCache::load();
        let now = now_secs();

        // Return cached value if it's fresh enough and we're not forcing.
        if !force
            && let Some(c) = &cached
            && now.saturating_sub(c.fetched_at) < UPDATE_CHECK_INTERVAL_SECS
        {
            return Some(c.version.clone());
        }

        let CurlResponse { status, body, etag } =
            curl(GITHUB_RELEASE_API, cached.as_ref().map(|c| c.etag.as_str()))?;

        // 304 Not Modified, refresh timestamp
        if status == 304 {
            if let Some(c) = cached {
                let refreshed = UpdateCache {
                    fetched_at: now,
                    ..c
                };
                refreshed.save();
                return Some(refreshed.version);
            }
            return None;
        }

        if !(200..300).contains(&status) {
            return None;
        }

        let json: serde_json::Value = serde_json::from_str(&body).ok()?;
        let tag = json["tag_name"].as_str()?;
        let version = tag.trim_start_matches('v').to_string();

        if let Some(mut cache) = cached {
            cache.etag = etag;
            cache.fetched_at = now;
            cache.version = version.clone();
            cache.save();
        }

        Some(version)
    }
}

mod format {
    use frontend::{formatter, lexer, parser};

    use super::*;

    pub fn format(target_paths: Vec<PathBuf>, args: FormatArgs) -> Result<(), String> {
        // Lexing
        let sources = target_paths
            .into_iter()
            .map(|p| {
                let src = std::fs::read_to_string(&p)
                    .map_err(|e| format!("Failed to read source file {}: {}", p.display(), e))?;

                Ok((src, p))
            })
            .collect::<Result<Vec<(String, PathBuf)>, String>>()
            .map_err(|e| {
                tracing::error!("{}", e);
                "Failed to read source files".to_string()
            })?;

        let (results, file_table) = lexer::lex(sources.iter().map(|(src, path)| LexTarget {
            src: src.as_str(),
            path: path.clone(),
        }))
        .unwrap_or_else(|(errors, file_table)| {
            errors.display_error(&file_table);
            std::process::exit(1);
        });

        let mut any_diff = false;

        // Parsing
        for lex in &results {
            let (src, path) = file_table.resolve(lex.file_id);

            let ast = parser::parse(std::slice::from_ref(lex), &file_table).unwrap_or_else(|err| {
                err.display_error(&file_table);
                std::process::exit(1);
            });

            let formatted = formatter::format(&ast, &lex.comment_map, src);

            if args.check {
                // TODO: sophisticated diffing
                if formatted != src {
                    any_diff = true;
                    tracing::error!("{}: not formatted", path.display());
                }
            } else {
                std::fs::write(path, formatted.as_bytes())
                    .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
                tracing::info!("Formatted {}", path.display());
            }
        }

        if any_diff {
            return Err("formatting check failed: some files are not formatted".into());
        }

        Ok(())
    }
}
