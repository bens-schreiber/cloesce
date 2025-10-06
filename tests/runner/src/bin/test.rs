use clap::{Parser, Subcommand};
use glob::glob;
use runner::Fixture;

use std::thread;

#[derive(Parser)]
#[command(name = "test", version = "0.0.1")]
struct Cli {
    #[arg(short = 'c', long = "check", global = true)]
    check_only: bool,

    #[arg(long, default_value = "http://localhost:5002/api")]
    domain: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Clone)]
enum Commands {
    Regression,
    RunFail(RunFail),
}

#[derive(Parser, Clone)]
struct RunFail {
    #[command(subcommand)]
    subcommand: RunFailSubcommands,
}

#[derive(Subcommand, Clone)]
enum RunFailSubcommands {
    Extractor,
}

/// Runs the regression tests, passing each fixture through the entire compilation process.
fn main() {
    let cli = Cli::parse();

    let pattern = match &cli.command {
        Commands::Regression => "../fixtures/regression/*/seed__*",
        Commands::RunFail(rf) => match rf.subcommand {
            RunFailSubcommands::Extractor => "../fixtures/run_fail/extractor/*/*.ts",
        },
    };

    let fixtures = glob(pattern)
        .expect("valid glob pattern")
        .filter_map(Result::ok)
        .filter(|p| p.is_file())
        .map(|p| Fixture::new(p, cli.check_only))
        .collect::<Vec<_>>();

    // todo: thread pool
    let handles: Vec<_> = fixtures
        .into_iter()
        .map(|fixture| {
            let domain = cli.domain.clone();
            let command = cli.command.clone();
            thread::spawn(move || -> bool {
                match command {
                    Commands::Regression => run_regression(fixture, &domain),
                    Commands::RunFail(rf) => match rf.subcommand {
                        RunFailSubcommands::Extractor => run_extractor(fixture),
                    },
                }
                .unwrap_or_else(|err| err)
            })
        })
        .collect();

    let mut changed = false;
    for handle in handles {
        changed |= handle.join().expect("thread to exit without panicing");
    }

    if changed {
        if cli.check_only {
            panic!("Found a difference in snapshot files.");
        } else {
            println!("Run `cargo run --bin update` to update the snapshot tests");
        }

        return;
    }

    println!("No changes found.");
}

fn run_regression(fixture: Fixture, domain: &str) -> Result<bool, bool> {
    let (cidl_changed, cidl_path) = fixture.extract_cidl().map_err(|e| e.0)?;
    let (wrangler_changed, wrangler_path) = fixture.generate_wrangler().map_err(|e| e.0)?;
    let d1_changed = fixture.generate_d1(&cidl_path)?;
    let workers_changed = fixture.generate_workers(&cidl_path, &wrangler_path, domain)?;
    let client_changed = fixture.generate_client(&cidl_path, domain)?;

    Ok(cidl_changed | wrangler_changed | d1_changed | workers_changed | client_changed)
}

fn run_extractor(fixture: Fixture) -> Result<bool, bool> {
    fixture
        .extract_cidl()
        .map(|(changed, _path)| changed)
        .map_err(|e| e.0)
}
