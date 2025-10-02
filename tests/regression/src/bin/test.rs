use glob::glob;
use similar::TextDiff;

use std::{
    fs::{self},
    panic,
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
    thread,
};

const DOMAIN: &str = "http://localhost:5002/api";

static CHECK_MODE: OnceLock<bool> = OnceLock::new();
fn is_check_mode() -> bool {
    *CHECK_MODE.get_or_init(|| std::env::args().any(|arg| arg == "--check"))
}

fn main() {
    let pattern = "../fixtures/*/";
    let fixtures = glob(pattern)
        .expect("valid glob pattern")
        .filter_map(Result::ok)
        .filter(|p| p.is_dir())
        .map(Fixture::new)
        .collect::<Vec<_>>();

    // todo: thread pool
    let handles: Vec<_> = fixtures
        .into_iter()
        .map(|fixture| {
            thread::spawn(move || {
                let (cidl_changed, cidl_path) = fixture.extract_cidl();
                let (wrangler_changed, wrangler_path) = fixture.generate_wrangler();
                let d1_changed = fixture.generate_d1(&cidl_path);
                let workers_changed = fixture.generate_workers(&cidl_path, &wrangler_path);
                let client_changed = fixture.generate_client(&cidl_path);

                let changed =
                    cidl_changed | wrangler_changed | d1_changed | workers_changed | client_changed;

                if changed {
                    println!("Run `cargo run --bin update` to update the snapshot tests\n\n");
                } else {
                    println!("No changes found for {:?}", fixture.path)
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

/// A temporary file inside of the fixture path
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn new(fixture_path: &PathBuf, name: &str) -> Self {
        if !fixture_path.exists() {
            fs::create_dir_all(fixture_path).unwrap();
        }

        let path = fixture_path.join(format!("tmp.{name}"));

        std::fs::File::create(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> PathBuf {
        self.path.canonicalize().unwrap()
    }

    fn trim_name(&self) -> String {
        self.path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .strip_prefix("tmp.")
            .unwrap()
            .to_string()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

// TODO: hardcoded for TS
struct Fixture {
    path: PathBuf,
}

impl Fixture {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn extract_cidl(&self) -> (bool, PathBuf) {
        let tmp = TempFile::new(&self.path, "cidl.json");
        self.run_command(
            Command::new("node")
                .arg("../../src/extractor/ts/dist/cli.js")
                .arg("--location")
                .arg(&self.path)
                .arg("--out")
                .arg(&tmp.path),
            "Node command failed",
        );

        Self::read_temp_and_diff(&self.path, tmp)
    }

    fn generate_wrangler(&self) -> (bool, PathBuf) {
        let tmp = TempFile::new(&self.path, "wrangler.toml");
        self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("wrangler")
                .arg(tmp.path())
                .current_dir("../../src/generator"),
            "Cargo run generate wrangler failed",
        );
        Self::read_temp_and_diff(&self.path, tmp)
    }

    fn generate_d1(&self, cidl: &Path) -> bool {
        let cidl_path = cidl.canonicalize().unwrap();
        let tmp_sqlite = TempFile::new(&self.path.join("migrations"), "migrations.sql");

        self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("d1")
                .arg(&cidl_path)
                .arg(tmp_sqlite.path())
                .current_dir("../../src/generator"),
            "Cargo run generate d1 failed",
        );

        Self::read_temp_and_diff(&self.path.join("migrations"), tmp_sqlite).0
    }

    fn generate_workers(&self, cidl: &Path, wrangler: &Path) -> bool {
        let cidl_path = cidl.canonicalize().unwrap();
        let wrangler_path = wrangler.canonicalize().unwrap();
        let tmp_workers = TempFile::new(&self.path, "workers.ts");

        self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("workers")
                .arg(&cidl_path)
                .arg(tmp_workers.path())
                .arg(&wrangler_path)
                .arg(DOMAIN)
                .current_dir("../../src/generator"),
            "Cargo run generate workers failed",
        );

        Self::read_temp_and_diff(&self.path, tmp_workers).0
    }

    fn generate_client(&self, cidl: &Path) -> bool {
        let cidl_path = cidl.canonicalize().unwrap();
        let tmp = TempFile::new(&self.path, "client.ts");

        self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("client")
                .arg(&cidl_path)
                .arg(tmp.path())
                .arg(DOMAIN)
                .current_dir("../../src/generator"),
            "Cargo run generate client failed",
        );

        Self::read_temp_and_diff(&self.path, tmp).0
    }

    fn run_command(&self, command: &mut Command, error_msg: &str) {
        let output = command.output().expect("Failed to execute command");

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            panic!("{} for {:?}:\n{}", error_msg, self.path, stderr);
        }
    }

    fn read_temp_and_diff(path: &Path, tmp: TempFile) -> (bool, PathBuf) {
        let contents = fs::read(&tmp.path).expect("temp file to be readable");
        diff_file(
            path,
            &tmp.trim_name(),
            String::from_utf8_lossy(&contents).to_string(),
        )
    }
}

/// Compares unified file diffs, creating a `.new` snapshot file if a diff is found
///
/// Returns if a `.new` file created
fn diff_file(fixture_path: &Path, name: &str, new_contents: String) -> (bool, PathBuf) {
    let old_path = fixture_path.join(name);
    let new_path = old_path.with_file_name(format!(
        "snap___{}",
        old_path.file_name().unwrap().to_string_lossy()
    ));

    let old_contents = fs::read(&old_path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();

    // No changes, wrote nothing
    if old_contents == new_contents {
        return (false, old_path);
    }

    let diff = TextDiff::from_lines(&old_contents, &new_contents);
    let unified_diff = diff
        .unified_diff()
        .context_radius(3)
        .header(old_path.to_str().unwrap(), new_path.to_str().unwrap())
        .to_string();

    if !unified_diff.trim().is_empty() {
        for line in unified_diff.lines() {
            if line.starts_with('+') && !line.starts_with("+++") {
                println!("\x1b[32m{}\x1b[0m", line); // green
            } else if line.starts_with('-') && !line.starts_with("---") {
                println!("\x1b[31m{}\x1b[0m", line); // red
            } else if line.starts_with('@') {
                println!("\x1b[36m{}\x1b[0m", line); // cyan
            } else {
                println!("\x1b[90m{}\x1b[0m", line); // gray
            }
        }
    }

    if is_check_mode() {
        panic!("Snapshot mismatch detected at {:?}", old_path);
    }

    // Wrote a diff'd file or new file
    fs::write(&new_path, new_contents).expect("path to be written");
    (true, new_path)
}
