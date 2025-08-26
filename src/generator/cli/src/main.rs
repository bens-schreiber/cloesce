use clap::{Parser, Subcommand, command};

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
        file_path: String,
    },

    Generate {
        #[command(subcommand)]
        target: GenerateTarget,
    },
}

#[derive(Subcommand)]
enum GenerateTarget {
    Sql { file_path: String },

    WorkersApi { file_path: String },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate { file_path } => match CidlSpec::from_file_path(&file_path) {
            Ok(spec) => println!("Loaded project: {}", spec.project_name),
            Err(e) => eprintln!("Error: {}", e),
        },

        Commands::Generate { target } => match target {
            GenerateTarget::Sql { file_path: _ } => {
                todo!("generate SQL");
            }

            GenerateTarget::WorkersApi { file_path: _ } => {
                todo!("generate workers api");
            }
        },
    }
}
