use std::{io::Write, path::PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, command};

use cli::WranglerFormat;
use common::CidlSpec;
use d1::D1Generator;
use workers::WorkersGenerator;

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
        wrangler_path: Option<PathBuf>,
        output_path: Option<PathBuf>,
    },
    Client {
        cidl_path: PathBuf,
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
                let mut sqlite_file = std::fs::File::create(sqlite_path)?;
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

                // region: Update Wrangler
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
                // endregion: Update Wrangler

                // region: Generate SQL
                {
                    let generated_sqlite =
                        d1gen.sqlite().context("Failed to generate sqlite file")?;
                    sqlite_file
                        .write(generated_sqlite.as_bytes())
                        .context("Failed to write to sqlite file")?;
                }
                // endregion: Generate SQL
            }
            GenerateTarget::Workers {
                cidl_path,
                wrangler_path,
                output_path,
            } => {
                let cidl = cidl_from_path(cidl_path)?;

                let wrangler = match wrangler_path {
                    Some(ref wrangler_path) => WranglerFormat::from_path(wrangler_path)
                        .context("Failed to open wrangler file")?,
                    _ => {
                        // Default to an empty TOML if the path is not given.
                        WranglerFormat::Toml(toml::from_str("").unwrap())
                    }
                };

                let wrangler_spec = wrangler
                    .as_spec()
                    .context("Failed to validate Wrangler file")?;

                let generated_code = WorkersGenerator::generate(&cidl, &wrangler_spec)
                    .context("Failed to generate Workers API code")?;

                // Write to file or stdout
                match output_path {
                    Some(output_path) => {
                        let mut output_file = std::fs::File::create(output_path)
                            .context("Failed to create output file")?;
                        output_file
                            .write_all(generated_code.as_bytes())
                            .context("Failed to write Workers API code to file")?;
                        println!("Workers API code generated successfully!");
                    }
                    None => {
                        // Output to stdout if no file specified
                        println!("{}", generated_code);
                    }
                }
            }
            GenerateTarget::Client { cidl_path } => {
                let spec = cidl_from_path(cidl_path)?;
                println!("{}", client::generate_client_api(spec));
            }
        },
    }

    Ok(())
}

fn cidl_from_path(cidl_path: PathBuf) -> Result<CidlSpec> {
    let cidl_contents = std::fs::read_to_string(cidl_path).context("Failed to read cidl file")?;
    serde_json::from_str::<CidlSpec>(&cidl_contents).context("Failed to validate cidl")
}
