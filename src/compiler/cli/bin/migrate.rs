use std::{
    io::{Read, Write},
    panic,
    path::PathBuf,
};

use ast::MigrationsAst;
use clap::{Parser, arg, command};
use cli::open_file_or_create;
use codegen::{
    migrations::{MigrationsDilemma, MigrationsGenerator, MigrationsIntent},
    wrangler::WranglerGenerator,
};

#[derive(Parser)]
#[command(name = "compile", version = "0.0.3")]
struct Args {
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
}

fn main() {
    match panic::catch_unwind(migrate) {
        Ok(Ok(())) => std::process::exit(0),
        Ok(Err(e)) => {
            tracing::error!("An error occurred during migrations: {e}");
            std::process::exit(1);
        }
        Err(e) => {
            tracing::error!("An uncaught error occurred during migrations: {:?}", e);
            std::process::abort();
        }
    }
}

fn migrate() -> Result<(), String> {
    let args = Args::parse();
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

        std::fs::create_dir_all(&migrations_dir).expect("Failed to create migrations directory");
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

        let mut migrated_cidl_file = open_file_or_create(&migrated_cidl_path);
        let mut migrated_sql_file = open_file_or_create(&migrated_sql_path);

        let lm_ast = last_migrated_cidl_path
            .map(|p| MigrationsAst::from_json(&p))
            .transpose()?;

        // Migrate only the models with the specified D1 binding
        let mut ast = MigrationsAst::from_json(&args.cidl_path)?;
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
