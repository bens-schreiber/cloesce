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
        Commands::Regression => "../fixtures/regression/*/seed__*",
        Commands::RunFail => "../fixtures/run_fail/*/*/*.ts",
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

    let (cidl_changed, cidl_path) = run_step(fixture.extract_cidl())?;
    let (wrangler_changed, wrangler_path) = run_step(fixture.generate_wrangler())?;
    let (d1_changed, _) = run_step(fixture.generate_d1(&cidl_path))?;
    let (workers_changed, _) =
        run_step(fixture.generate_workers(&cidl_path, &wrangler_path, domain))?;
    let (client_changed, _) = run_step(fixture.generate_client(&cidl_path, domain))?;

    if matches!(command, Commands::RunFail) {
        for p in cleanup {
            let _ = fs::remove_file(p);
        }
    }

    Ok(cidl_changed | wrangler_changed | d1_changed | workers_changed | client_changed)
}
