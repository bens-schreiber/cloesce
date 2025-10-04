use std::{io::Write, panic, path::PathBuf};

use clap::{Parser, Subcommand, command};

use common::{
    CloesceAst,
    err::{GeneratorError, Result},
};
use workers::WorkersGenerator;
use wrangler::WranglerFormat;

#[derive(Parser)]
#[command(name = "generate", version = "0.0.1")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Validate {
        cidl_path: PathBuf,
    },
    Generate {
        #[command(subcommand)]
        target: GenerateTarget,
    },
}

#[derive(Subcommand)]
enum GenerateTarget {
    Wrangler {
        wrangler_path: PathBuf,
    },
    D1 {
        cidl_path: PathBuf,
        sqlite_path: PathBuf,
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
}

fn main() {
    let result = panic::catch_unwind(cli);

    if let Ok(Ok(())) = result {
        return;
    }

    if let Ok(Err(e)) = result {
        let GeneratorError {
            description,
            suggestion,
            kind,
            phase,
            context,
        } = e;

        eprintln!(
            r#"==== CLOESCE ERROR ====
Error [{kind:?}]: {description}
Phase: {phase:?}
Context: {context}
Suggested fix: {suggestion}"#
        );
        return;
    }

    if let Err(panic_info) = result {
        eprintln!("==== GENERATOR PANIC CAUGHT ====");
        let msg = panic_info
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| panic_info.downcast_ref::<String>().map(|s| s.as_str()))
            .unwrap_or("Panic occurred but couldn't extract info.");
        eprintln!("Panic info: {}", msg);
    }
}

fn cli() -> Result<()> {
    match Cli::parse().command {
        Commands::Validate { cidl_path } => {
            let cidl = ast_from_path(cidl_path);
            cidl.validate_types()?;
            println!("Ok.")
        }
        Commands::Generate { target } => match target {
            GenerateTarget::Wrangler { wrangler_path } => {
                let mut wrangler = WranglerFormat::from_path(&wrangler_path);
                let mut spec = wrangler.as_spec();
                spec.generate_defaults();

                let wrangler_file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&wrangler_path)
                    .expect("Wrangler file to be opened");

                wrangler.update(spec, wrangler_file);
            }
            GenerateTarget::D1 {
                cidl_path,
                sqlite_path,
            } => {
                let mut sqlite_file = create_file_and_dir(&sqlite_path);
                let ast = ast_from_path(cidl_path);
                ast.validate_types()?;

                let generated_sqlite = d1::generate_sql(&ast.models)?;

                sqlite_file
                    .write_all(generated_sqlite.as_bytes())
                    .expect("SQL file to be written");
            }
            GenerateTarget::Workers {
                cidl_path,
                workers_path,
                wrangler_path,
                domain,
            } => {
                let ast = ast_from_path(cidl_path);
                ast.validate_types()?;

                let mut file = create_file_and_dir(&workers_path);

                let wrangler = WranglerFormat::from_path(&wrangler_path);

                let workers =
                    WorkersGenerator::create(ast, wrangler.as_spec(), domain, &workers_path)?;

                file.write_all(workers.as_bytes())
                    .expect("Failed to write workers file");
            }
            GenerateTarget::Client {
                cidl_path,
                client_path,
                domain,
            } => {
                let ast = ast_from_path(cidl_path);
                ast.validate_types()?;

                let mut file = create_file_and_dir(&client_path);

                file.write_all(client::generate_client_api(ast, domain).as_bytes())
                    .expect("Failed to write client file");
            }
        },
    }

    Ok(())
}

fn create_file_and_dir(path: &PathBuf) -> std::fs::File {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("Failed to create parent dir");
    }
    std::fs::File::create(path).expect("Failed to create file")
}

fn ast_from_path(cidl_path: PathBuf) -> CloesceAst {
    let cidl_contents = std::fs::read_to_string(cidl_path).expect("Failed to read cidl file");
    serde_json::from_str::<CloesceAst>(&cidl_contents).expect("Failed to validate cidl")
}
