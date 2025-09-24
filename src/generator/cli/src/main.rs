use std::{io::Write, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, command};

use common::{CidlSpec, WranglerFormat};
use d1::D1Generator;
use workers::WorkersFactory;

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
    D1 {
        cidl_path: PathBuf,
        sqlite_path: PathBuf,
        wrangler_path: Option<PathBuf>,
    },
    Workers {
        cidl_path: PathBuf,
        workers_path: PathBuf,
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
            cidl_from_path(cidl_path)?;
            println!("Ok.")
        }
        Commands::Generate { target } => match target {
            GenerateTarget::D1 {
                cidl_path,
                wrangler_path,
                sqlite_path,
            } => {
                let mut sqlite_file = create_file_and_dir(sqlite_path)?;
                let cidl = cidl_from_path(cidl_path)?;

                let mut wrangler = match wrangler_path {
                    Some(ref wrangler_path) => WranglerFormat::from_path(wrangler_path)
                        .context("Failed to open wrangler file")?,
                    _ => {
                        // Default to an empty TOML if the path is not given.
                        WranglerFormat::Toml(toml::from_str("").unwrap())
                    }
                };

                let d1gen = D1Generator::new(
                    cidl,
                    wrangler
                        .as_spec()
                        .context("Failed to validate Wrangler file")?,
                );

                // Update wrangler config
                {
                    let updated_wrangler = d1gen.wrangler();
                    let wrangler_file = match wrangler_path {
                        Some(wrangler_path) => std::fs::File::create(wrangler_path)?,

                        // Default to an empty TOML if the path is not given.
                        _ => std::fs::File::create("./wrangler.toml")?,
                    };
                    wrangler
                        .update(&updated_wrangler, wrangler_file)
                        .context("Failed to update wrangler file")?;
                }

                // Generate SQL
                {
                    let generated_sqlite = d1gen.sql().context("Failed to generate sqlite file")?;
                    sqlite_file
                        .write(generated_sqlite.as_bytes())
                        .context("Failed to write to sqlite file")?;
                }
            }
            GenerateTarget::Workers {
                cidl_path,
                workers_path,
            } => {
                let cidl = cidl_from_path(cidl_path)?;
                let mut file =
                    create_file_and_dir(workers_path).context("Failed to open workers file")?;
                
                // since we're not making domains optional, we need to have a default value
                let generated = workers::WorkersFactory::new("localhost".to_string()).create(cidl);
                file.write_all(generated.as_bytes()).expect("failed to write output");
            }
            GenerateTarget::Client {
                cidl_path,
                client_path,
                domain,
            } => {
                let spec = cidl_from_path(cidl_path)?;
                let mut file =
                    create_file_and_dir(client_path).context("Failed to open client file")?;

                file.write(client::generate_client_api(spec, domain).as_bytes())
                    .context("Failed to write client file")?;
            }
        },
    }

    Ok(())
}

fn create_file_and_dir(path: PathBuf) -> Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = std::fs::File::create(path)?;
    Ok(file)
}

fn cidl_from_path(cidl_path: PathBuf) -> Result<CidlSpec> {
    let cidl_contents = std::fs::read_to_string(cidl_path).context("Failed to read cidl file")?;
    serde_json::from_str::<CidlSpec>(&cidl_contents).context("Failed to validate cidl")
}
