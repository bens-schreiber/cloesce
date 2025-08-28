use std::{io::Write, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, command};
use cli::WranglerFormat;
use d1::D1Generator;

use common::CidlSpec;

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
    WorkersApi {},
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Commands::Validate { cidl_path } => {
            let cidl_contents =
                std::fs::read_to_string(cidl_path).context("Failed to read cidl file")?;
            serde_json::from_str::<CidlSpec>(&cidl_contents).context("Failed to validate cidl")?;
            println!("Ok.")
        }
        Commands::Generate { target } => match target {
            GenerateTarget::D1 {
                cidl_path,
                wrangler_path,
                sqlite_path,
            } => {
                let mut sqlite_file = std::fs::File::create(sqlite_path)?;

                let cidl = {
                    let cidl_contents =
                        std::fs::read_to_string(cidl_path).context("Failed to read cidl file")?;

                    serde_json::from_str::<CidlSpec>(&cidl_contents)
                        .context("Failed to validate cidl")?
                };

                let mut wrangler = match wrangler_path {
                    Some(ref wrangler_path) => WranglerFormat::from_path(wrangler_path)
                        .context("Failed to open wrangler file")?,

                    // Default to an empty TOML if the path is not given.
                    _ => WranglerFormat::Toml(toml::from_str("").unwrap()),
                };

                let d1gen = D1Generator::new(
                    cidl,
                    wrangler
                        .as_spec()
                        .context("Failed to validate Wrangler file")?,
                );

                let generated_sqlite = d1gen
                    .gen_sqlite()
                    .context("Failed to generate sqlite file")?;
                sqlite_file
                    .write(generated_sqlite.as_bytes())
                    .context("Failed to write to sqlite file")?;

                let updated_wrangler = d1gen.gen_wrangler();
                let wrangler_file = match wrangler_path {
                    Some(wrangler_path) => std::fs::File::create(wrangler_path)?,

                    // Default to an empty TOML if the path is not given.
                    _ => std::fs::File::create("./wrangler.toml")?,
                };
                wrangler
                    .update(&updated_wrangler, wrangler_file)
                    .context("Failed to update wrangler file")?;
            }
            GenerateTarget::WorkersApi {} => {
                todo!("generate workers api");
            }
        },
    }

    Ok(())
}
