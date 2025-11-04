use std::{
    io::{self, Write},
    panic,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand, command};

use ast::{
    CloesceAst, MigrationsAst,
    err::{GeneratorErrorKind, Result},
};
use d1::{D1Generator, MigrationsDilemma, MigrationsIntent};
use workers::WorkersGenerator;
use wrangler::WranglerFormat;

#[derive(Parser)]
#[command(name = "generate", version = "0.0.3")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Validate {
        pre_cidl_path: PathBuf,
        cidl_path: PathBuf,
    },
    Generate {
        #[command(subcommand)]
        target: GenerateTarget,
    },
    Migrations {
        cidl_path: PathBuf,
        migrated_cidl_path: PathBuf,
        migrated_sql_path: PathBuf,
        last_migrated_cidl_path: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum GenerateTarget {
    Wrangler {
        wrangler_path: PathBuf,
    },
    Workers {
        cidl_path: PathBuf,
        workers_path: PathBuf,
        wrangler_path: PathBuf,
        domain: String,
    },
    Client {
        cidl_path: PathBuf,
        client_path: PathBuf,
        domain: String,
    },
    All {
        pre_cidl_path: PathBuf,
        cidl_path: PathBuf,
        wrangler_path: PathBuf,
        workers_path: PathBuf,
        client_path: PathBuf,
        client_domain: String,
        workers_domain: String,
    },
}

fn main() {
    match panic::catch_unwind(run_cli) {
        Ok(Ok(())) => std::process::exit(0),
        Ok(Err(e)) if matches!(e.kind, GeneratorErrorKind::InvalidInputFile) => {
            eprintln!(
                "==== CLOESCE ERROR ====\nInvalid generator file input: {}\n",
                e.context
            );
        }
        Ok(Err(e)) => {
            eprintln!(
                r#"==== CLOESCE ERROR ====
Error [{:?}]: {}
Phase: {:?}
Context: {}
Suggested fix: {}"#,
                e.kind, e.description, e.phase, e.context, e.suggestion
            );
        }
        Err(e) => {
            eprintln!("==== GENERATOR PANIC CAUGHT ====");
            let msg = e
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
                .unwrap_or("Panic occurred but couldn't extract info.");
            eprintln!("Panic info: {}", msg);
        }
    }

    std::process::exit(1);
}

fn run_cli() -> Result<()> {
    match Cli::parse().command {
        Commands::Validate {
            pre_cidl_path,
            cidl_path,
        } => {
            validate_cidl(&pre_cidl_path, &cidl_path)?;
            println!("Ok.");
        }
        Commands::Migrations {
            cidl_path,
            migrated_cidl_path,
            migrated_sql_path,
            last_migrated_cidl_path,
        } => {
            let mut migrated_cidl_file = create_file_and_dir(&migrated_cidl_path)?;
            let mut migrated_sql_file = create_file_and_dir(&migrated_sql_path)?;

            let lm_ast = last_migrated_cidl_path
                .map(|p| MigrationsAst::from_json(&p))
                .transpose()?;
            let ast = MigrationsAst::from_json(&cidl_path)?;

            let generated_sql = D1Generator::migrate(&ast, lm_ast.as_ref(), &MigrationsCli);

            migrated_cidl_file
                .write_all(ast.to_json().as_bytes())
                .expect("Could not write to file");
            migrated_sql_file
                .write_all(generated_sql.as_bytes())
                .expect("Could not write to file");
        }
        Commands::Generate { target } => match target {
            GenerateTarget::Wrangler { wrangler_path } => generate_wrangler(&wrangler_path)?,
            GenerateTarget::Workers {
                cidl_path,
                workers_path,
                wrangler_path,
                domain,
            } => {
                let ast = CloesceAst::from_json(&cidl_path)?;
                generate_workers(&ast, &workers_path, &wrangler_path, &domain)?
            }
            GenerateTarget::Client {
                cidl_path,
                client_path,
                domain,
            } => {
                let ast = CloesceAst::from_json(&cidl_path)?;
                generate_client(&ast, &client_path, &domain)?
            }
            GenerateTarget::All {
                pre_cidl_path,
                cidl_path,
                wrangler_path,
                workers_path,
                client_path,
                client_domain,
                workers_domain,
            } => {
                let ast = validate_cidl(&pre_cidl_path, &cidl_path)?;
                println!("Validation complete.");

                generate_wrangler(&wrangler_path)?;
                println!("Wrangler generated.");

                generate_workers(&ast, &workers_path, &wrangler_path, &workers_domain)?;
                println!("Workers generated.");

                generate_client(&ast, &client_path, &client_domain)?;
                println!("Client generated.");

                println!("All generation steps completed successfully!");
            }
        },
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
            MigrationsDilemma::RenameOrDropAttribute {
                model_name,
                attribute_name,
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

        let mut line = String::new();
        if io::stdin().read_line(&mut line).is_err() {
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
                io::stdout().flush().unwrap();

                let mut input = String::new();
                if io::stdin().read_line(&mut input).is_err() {
                    eprintln!("Error reading input. Aborting migrations.");
                    std::process::abort();
                }

                let idx = input.trim().parse::<usize>().unwrap_or_else(|_| {
                    eprintln!("Invalid selection. Aborting migrations.");
                    std::process::abort();
                });

                if idx >= options.len() {
                    eprintln!("Index out of range. Aborting migrations.");
                    std::process::abort();
                }

                Some(idx)
            }
            _ => {
                eprintln!("Invalid option. Aborting migrations.");
                std::process::abort();
            }
        }
    }
}

fn validate_cidl(pre_cidl_path: &Path, cidl_path: &Path) -> Result<CloesceAst> {
    let mut ast = CloesceAst::from_json(pre_cidl_path)?;
    ast.semantic_analysis()?;
    ast.set_merkle_hash();

    let mut cidl_file = create_file_and_dir(cidl_path)?;
    cidl_file
        .write_all(ast.to_json().as_bytes())
        .expect("file to be written");

    Ok(ast)
}

fn generate_wrangler(wrangler_path: &Path) -> Result<()> {
    let mut wrangler = WranglerFormat::from_path(wrangler_path);
    let mut spec = wrangler.as_spec();
    spec.generate_defaults();

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

    wrangler.update(spec, wrangler_file);
    Ok(())
}

fn generate_workers(
    ast: &CloesceAst,
    workers_path: &Path,
    wrangler_path: &Path,
    domain: &str,
) -> Result<()> {
    let mut file = create_file_and_dir(workers_path)?;
    let wrangler = WranglerFormat::from_path(wrangler_path);

    let workers =
        WorkersGenerator::create(ast, wrangler.as_spec(), domain.to_string(), workers_path)?;
    file.write_all(workers.as_bytes())
        .expect("Could not write to file");
    Ok(())
}

fn generate_client(ast: &CloesceAst, client_path: &Path, domain: &str) -> Result<()> {
    let mut file = create_file_and_dir(client_path)?;
    file.write_all(client::generate_client_api(ast, domain.to_string()).as_bytes())
        .expect("Could not write to file");
    Ok(())
}

fn create_file_and_dir(path: &Path) -> Result<std::fs::File> {
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
