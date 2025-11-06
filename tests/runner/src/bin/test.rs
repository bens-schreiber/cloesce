use clap::{Parser, Subcommand};
use glob::glob;
use runner::{DiffOpts, Fixture};

use std::{fs, path::PathBuf, thread};

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
    RunFail,
}

/// Runs the regression tests, passing each fixture through the entire compilation process.
fn main() {
    let cli = Cli::parse();

    let pattern = match &cli.command {
        Commands::Regression => &format!("../fixtures/regression/{}/seed__*", cli.fixture),
        Commands::RunFail => &format!("../fixtures/run_fail/*/{}/*.ts", cli.fixture),
    };

    let fixtures = glob(pattern)
        .expect("valid glob pattern")
        .filter_map(Result::ok)
        .filter(|p| p.is_file())
        .map(|p| {
            let opt = match &cli.command {
                Commands::Regression => DiffOpts::All,
                Commands::RunFail => DiffOpts::FailOnly,
            };
            Fixture::new(p, opt)
        })
        .collect::<Vec<_>>();

    // todo: thread pool
    let handles: Vec<_> = fixtures
        .into_iter()
        .map(|fixture| {
            let domain = cli.domain.clone();
            let command = cli.command.clone();
            thread::spawn(move || -> bool {
                run_integration_test(fixture, &domain, command).unwrap_or_else(|err| err)
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

fn run_integration_test(fixture: Fixture, domain: &str, command: Commands) -> Result<bool, bool> {
    let mut cleanup = Vec::new();

    let mut run_step =
        |res: Result<(bool, PathBuf), (bool, PathBuf)>| -> Result<(bool, PathBuf), bool> {
            match res {
                Ok((changed, path)) => {
                    cleanup.push(path.clone());
                    Ok((changed, path))
                }
                Err((err, _)) => {
                    if matches!(command, Commands::RunFail) {
                        for p in &cleanup {
                            let _ = fs::remove_file(p);
                        }
                        return Err(err);
                    }

                    Err(err)
                }
            }
        };

    let (pre_cidl_changed, pre_cidl_path) = run_step(fixture.extract_cidl())?;
    let (cidl_changed, cidl_path) = run_step(fixture.validate_cidl(&pre_cidl_path))?;
    let (wrangler_changed, wrangler_path) = run_step(fixture.generate_wrangler())?;
    let (workers_changed, _) =
        run_step(fixture.generate_workers(&cidl_path, &wrangler_path, domain))?;
    let (client_changed, _) = run_step(fixture.generate_client(&cidl_path, domain))?;
    let migrations_changed = {
        let (s1, s2) = fixture.migrate(&cidl_path);
        let (migrated_cidl_changed, _) = run_step(s1)?;
        let (migrated_sql_changed, _) = run_step(s2)?;

        migrated_cidl_changed | migrated_sql_changed
    };

    if matches!(command, Commands::RunFail) {
        for p in cleanup {
            let _ = fs::remove_file(p);
        }
    }

    Ok(pre_cidl_changed
        | cidl_changed
        | wrangler_changed
        | workers_changed
        | client_changed
        | migrations_changed)
}
