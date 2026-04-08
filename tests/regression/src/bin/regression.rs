use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    process::Command,
};

use clap::Parser;
use futures::future::join_all;
use glob::glob;

use regression::Fixture;

#[derive(Parser)]
#[command(name = "regression", version = "0.0.1")]
struct Cli {
    #[arg(short = 'c', long = "check", global = true)]
    check: bool,

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
        "{}/tests/e2e/fixtures/{}/schema.cloesce",
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

    tracing::info!("Building compiler...");
    let compiler_dir = project_root.join("src/compiler");
    let cmd = Command::new("cargo")
        .current_dir(&compiler_dir)
        .args(["build", "--features", "regression-tests", "--release"])
        .status();
    match cmd {
        Ok(status) if status.success() => {}
        Ok(status) => {
            panic!("Failed to build compiler. Exit code: {}", status);
        }
        Err(err) => {
            panic!("Failed to execute cargo build: {}", err);
        }
    }
    tracing::info!("Finished building compiler.");

    let mut tasks = Vec::with_capacity(fixtures.len());
    for fixture in fixtures {
        let domain = create_cloesce_config(&fixture);
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
    let (generated_changed, cidl_path, wrangler_path) = match fixture.compile(domain) {
        Ok(res) => res,
        Err(err) => {
            eprintln!(
                "Error generating files for fixture {}: {}",
                fixture.fixture_id, err
            );
            return Err(true);
        }
    };

    let (migrated_sql_changed, migrated_cidl_changed) =
        match fixture.migrate(&cidl_path, &wrangler_path) {
            Ok(res) => res,
            Err(err) => {
                eprintln!(
                    "Error migrating files for fixture {}: {}",
                    fixture.fixture_id, err
                );
                return Err(true);
            }
        };

    tracing::info!(
        "Finished regression test for fixture {}",
        fixture.fixture_id
    );

    Ok(generated_changed | migrated_cidl_changed | migrated_sql_changed)
}

fn create_cloesce_config(fixture: &Fixture) -> String {
    let mut hasher = DefaultHasher::new();
    fixture.fixture_id.hash(&mut hasher);
    let port_seed = hasher.finish() % 1000;

    let domain = format!("http://localhost:{}/api", 5000 + port_seed);
    let config_source = format!(
        r#"{{
    "src_paths": ["./"],
    "workers_url": "{}",
    "out_path": "."
}}
"#,
        domain
    );

    // Write the config to cloesce.jsonc in the fixture directory
    let config_path = fixture
        .path
        .parent()
        .expect("fixture parent to exist")
        .join("cloesce.jsonc");
    std::fs::write(&config_path, config_source).unwrap_or_else(|err| {
        panic!(
            "Failed to write config for fixture {}: {}",
            fixture.fixture_id, err
        )
    });

    domain
}
