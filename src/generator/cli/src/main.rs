use std::{io::Write, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, command};

use common::CloesceAst;
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

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Validate { cidl_path } => {
            let cidl = ast_from_path(cidl_path)?;
            cidl.validate_types()?;
            println!("Ok.")
        }
        Commands::Generate { target } => match target {
            GenerateTarget::Wrangler { wrangler_path } => {
                let mut wrangler = WranglerFormat::from_path(&wrangler_path)
                    .context("Failed to open wrangler file")?;
                let mut spec = wrangler.as_spec()?;
                spec.generate_defaults();

                let wrangler_file = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .open(&wrangler_path)?;

                wrangler
                    .update(spec, wrangler_file)
                    .context("Failed to update wrangler file")?;
            }
            GenerateTarget::D1 {
                cidl_path,
                sqlite_path,
            } => {
                let mut sqlite_file = create_file_and_dir(&sqlite_path)?;
                let ast = ast_from_path(cidl_path)?;
                ast.validate_types()?;

                let generated_sqlite =
                    d1::generate_sql(&ast.models).context("Failed to generate sqlite file")?;

                sqlite_file
                    .write(generated_sqlite.as_bytes())
                    .context("Failed to write to sqlite file")?;
            }
            GenerateTarget::Workers {
                cidl_path,
                workers_path,
                wrangler_path,
                domain,
            } => {
                let ast = ast_from_path(cidl_path)?;
                ast.validate_types()?;

                let mut file =
                    create_file_and_dir(&workers_path).context("Failed to open workers file")?;

                let wrangler = WranglerFormat::from_path(&wrangler_path)
                    .context("Failed to open wrangler file")?;

                let workers =
                    WorkersGenerator::create(ast, wrangler.as_spec()?, domain, &workers_path)?;

                file.write(workers.as_bytes())
                    .context("Failed to write workers file")?;
            }
            GenerateTarget::Client {
                cidl_path,
                client_path,
                domain,
            } => {
                let ast = ast_from_path(cidl_path)?;
                ast.validate_types()?;

                let mut file =
                    create_file_and_dir(&client_path).context("Failed to open client file")?;

                file.write(client::generate_client_api(ast, domain).as_bytes())
                    .context("Failed to write client file")?;
            }
        },
    }

    Ok(())
}

fn create_file_and_dir(path: &PathBuf) -> Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(path)?;
    Ok(file)
}

fn ast_from_path(cidl_path: PathBuf) -> Result<CloesceAst> {
    let cidl_contents = std::fs::read_to_string(cidl_path).context("Failed to read cidl file")?;
    serde_json::from_str::<CloesceAst>(&cidl_contents).context("Failed to validate cidl")
}
