use std::process::Command;

use clap::Parser;
use futures::future::join_all;
use glob::glob;
use regression::Fixture;

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
#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Find project root (two levels up from tests/regression)
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let project_root = std::path::Path::new(manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .expect("Failed to find project root");

    let pattern = format!(
        "{}/tests/e2e/fixtures/{}/seed__*",
        project_root.display(),
        cli.fixture
    );

    let fixtures = glob(&pattern)
        .expect("valid glob pattern")
        .filter_map(Result::ok)
        .filter(|p| p.is_file())
        .map(Fixture::new)
        .collect::<Vec<_>>();

    let subscriber = tracing_subscriber::FmtSubscriber::builder().finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");

    // Build generator
    tracing::info!("Building generator...");
    let generator_dir = project_root.join("src/generator");
    let cmd = Command::new("cargo")
        .current_dir(&generator_dir)
        .args(["build", "--release"])
        .status();
    match cmd {
        Ok(status) if status.success() => {}
        Ok(status) => {
            panic!("Failed to build generator. Exit code: {}", status);
        }
        Err(err) => {
            panic!("Failed to execute cargo build: {}", err);
        }
    }
    tracing::info!("Finished building generator.");

    let mut tasks = Vec::with_capacity(fixtures.len());
    for fixture in fixtures {
        let domain = cli.domain.clone();

        tasks.push(tokio::task::spawn_blocking(move || {
            run_integration_test(fixture, &domain).unwrap_or(true)
        }));
    }

    let mut changed = false;
    let results = join_all(tasks).await;

    for result in results {
        changed |= result.expect("Task panicked");
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
    let (pre_cidl_changed, pre_cidl_path) = match fixture.extract_cidl() {
        Ok(res) => res,
        Err(err) => {
            eprintln!(
                "Error extracting CIDL for fixture {}: {}",
                fixture.fixture_id, err
            );
            return Err(true);
        }
    };

    let (generated_changed, cidl_path) = match fixture.generate_all(&pre_cidl_path, domain) {
        Ok(res) => res,
        Err(err) => {
            eprintln!(
                "Error generating files for fixture {}: {}",
                fixture.fixture_id, err
            );
            return Err(true);
        }
    };

    let (migrated_cidl_changed, migrated_sql_changed) = {
        let (s1, s2) = fixture.migrate(&cidl_path);
        let cidl = match s1 {
            Ok((res, _)) => res,
            Err(err) => {
                eprintln!(
                    "Error migrating CIDL for fixture {}: {}",
                    fixture.fixture_id, err
                );
                return Err(true);
            }
        };
        let sql = match s2 {
            Ok((res, _)) => res,
            Err(err) => {
                eprintln!(
                    "Error migrating SQL for fixture {}: {}",
                    fixture.fixture_id, err
                );
                return Err(true);
            }
        };

        (cidl, sql)
    };

    tracing::info!(
        "Finished regression test for fixture {}",
        fixture.fixture_id
    );

    Ok(pre_cidl_changed | generated_changed | migrated_cidl_changed | migrated_sql_changed)
}
