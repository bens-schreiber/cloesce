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

    #[arg(long, default_value = "*", global = true)]
    fixture: String,
}

#[derive(Subcommand, Clone)]
enum Commands {
    Regression,
}

/// Runs the regression tests, passing each fixture through the entire compilation process.
fn main() {
    let cli = Cli::parse();

    let pattern = match &cli.command {
        Commands::Regression => &format!("../fixtures/regression/{}/seed__*", cli.fixture),
    };

    let fixtures = glob(pattern)
        .expect("valid glob pattern")
        .filter_map(Result::ok)
        .filter(|p| p.is_file())
        .map(Fixture::new)
        .collect::<Vec<_>>();

    // todo: thread pool
    let handles: Vec<_> = fixtures
        .into_iter()
        .map(|fixture| {
            let domain = cli.domain.clone();
            thread::spawn(move || -> bool {
                run_integration_test(fixture, &domain).unwrap_or_else(|err| err)
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
fn run_integration_test(fixture: Fixture, domain: &str) -> Result<bool, bool> {
    let (pre_cidl_changed, pre_cidl_path) = fixture.extract_cidl().map_err(|(e, _)| e)?;
    let (generated_changed, cidl_path) = fixture
        .generate_all(&pre_cidl_path, domain, domain)
        .map_err(|(e, _)| e)?;
    let (s1, s2) = fixture.migrate(&cidl_path);
    let (migrated_cidl_changed, _) = s1.map_err(|(e, _)| e)?;
    let (migrated_sql_changed, _) = s2.map_err(|(e, _)| e)?;

    Ok(pre_cidl_changed | generated_changed | migrated_cidl_changed | migrated_sql_changed)
}
