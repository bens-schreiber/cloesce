use clap::Parser;
use glob::glob;
use runner::Fixture;

use std::thread;

#[derive(Parser)]
#[command(name = "regression", version = "0.0.1")]
struct Cli {
    #[arg(short = 'c', long = "check", global = true)]
    check: bool,

    #[arg(long, default_value = "http://localhost:5002/api")]
    domain: String,

    #[arg(long, default_value = "*", global = true)]
    fixture: String,
}

/// Runs the regression tests, passing each fixture through the entire compilation process.
fn main() {
    let cli = Cli::parse();

    let pattern = format!("../fixtures/regression/{}/seed__*", cli.fixture);

    let fixtures = glob(&pattern)
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
        if cli.check {
            panic!("Found a difference in snapshot files.");
        } else {
            println!(
                "Changes found. \n Run `cargo run --bin update` to update the snapshot tests or `cargo run --bin update -- -d` to delete them"
            );
        }

        return;
    }

    println!("No changes found.");
}

fn run_integration_test(fixture: Fixture, domain: &str) -> Result<bool, bool> {
    let (pre_cidl_changed, pre_cidl_path) = fixture.extract_cidl().map_err(|(e, _)| e)?;

    let (generated_changed, cidl_path) = fixture
        .generate_all(&pre_cidl_path, domain)
        .map_err(|(e, _)| e)?;

    let (migrated_cidl_changed, migrated_sql_changed) = {
        let (s1, s2) = fixture.migrate(&cidl_path);
        let (cidl, _) = s1.map_err(|(e, _)| e)?;
        let (sql, _) = s2.map_err(|(e, _)| e)?;
        (cidl, sql)
    };

    Ok(pre_cidl_changed | generated_changed | migrated_cidl_changed | migrated_sql_changed)
}
