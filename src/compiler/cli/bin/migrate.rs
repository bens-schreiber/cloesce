use std::{
    io::{Read, Write},
    panic,
    path::PathBuf,
};

use ast::MigrationsAst;
use clap::Parser;
use cli::open_file_or_create;
use codegen::wrangler::WranglerGenerator;
use migrations::{MigrationsDilemma, MigrationsGenerator, MigrationsIntent};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "migrate", version = "0.0.3")]
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


