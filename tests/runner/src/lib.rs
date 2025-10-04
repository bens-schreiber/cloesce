use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use similar::TextDiff;

// Compares unified file diffs, creating a `.new` snapshot file if a diff is found
///
/// Returns if a `.new` file created
pub fn diff_file(
    fixture_path: &Path,
    new_contents: String,
    fail: bool,
    check_mode: bool,
) -> (bool, PathBuf) {
    let name = if fail {
        format!("{}_fail.out", fixture_path.file_name().unwrap().display())
    } else {
        fixture_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string()
    };

    let new_path = fixture_path.with_file_name(format!("snap___{}", name));

    let old_contents = fs::read(&fixture_path.with_file_name(name))
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();

    // No changes, wrote nothing
    if old_contents == new_contents {
        return (false, fixture_path.to_path_buf());
    }

    let diff = TextDiff::from_lines(&old_contents, &new_contents);
    let unified_diff = diff
        .unified_diff()
        .context_radius(3)
        .header(fixture_path.to_str().unwrap(), new_path.to_str().unwrap())
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

    if check_mode {
        return (true, PathBuf::default());
    }

    // Wrote a diff'd file or new file
    fs::write(&new_path, new_contents).expect("path to be written");
    (true, new_path)
}

/// A temporary file inside of the fixture path
struct TempFile {
    pub path: PathBuf,
}

impl TempFile {
    fn new(fixture_path: &PathBuf, name: &str) -> Self {
        if !fixture_path.exists() {
            fs::create_dir_all(fixture_path).unwrap();
        }

        let path = fixture_path.with_file_name(format!("tmp.{name}"));

        std::fs::File::create(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> PathBuf {
        self.path.canonicalize().unwrap()
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub struct Fixture {
    pub path: PathBuf,
    pub check_only: bool,
}

impl Fixture {
    pub fn new(path: PathBuf, check_only: bool) -> Self {
        Self { path, check_only }
    }

    pub fn extract_cidl(&self) -> Result<(bool, PathBuf), (bool, PathBuf)> {
        let tmp = TempFile::new(&self.path, "cidl.json");
        let res = self.run_command(
            Command::new("node")
                .arg("../../src/extractor/ts/dist/cli.js")
                .arg("--location")
                .arg(&self.path)
                .arg("--out")
                .arg(&tmp.path)
                .arg("--truncateSourcePaths"),
        );

        match res {
            Ok(_) => Ok(self.read_temp_and_diff(&self.path, tmp)),
            Err(err) => Err(diff_file(&self.path, err, true, self.check_only)),
        }
    }

    pub fn generate_wrangler(&self) -> Result<(bool, PathBuf), (bool, PathBuf)> {
        let tmp = TempFile::new(&self.path, "wrangler.toml");
        let res = self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("wrangler")
                .arg(tmp.path())
                .current_dir("../../src/generator"),
        );

        match res {
            Ok(_) => Ok(self.read_temp_and_diff(&self.path, tmp)),
            Err(err) => Err(diff_file(&self.path, err, true, self.check_only)),
        }
    }

    pub fn generate_d1(&self, cidl: &Path) -> Result<bool, bool> {
        let cidl_path = cidl.canonicalize().unwrap();
        let tmp = TempFile::new(&self.path.join("migrations"), "migrations.sql");

        let res = self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("d1")
                .arg(&cidl_path)
                .arg(tmp.path())
                .current_dir("../../src/generator"),
        );

        match res {
            Ok(_) => Ok(self
                .read_temp_and_diff(&self.path.join("migrations"), tmp)
                .0),
            Err(err) => Err(diff_file(&self.path, err, true, self.check_only).0),
        }
    }

    pub fn generate_workers(
        &self,
        cidl: &Path,
        wrangler: &Path,
        domain: &str,
    ) -> Result<bool, bool> {
        let cidl_path = cidl.canonicalize().unwrap();
        let wrangler_path = wrangler.canonicalize().unwrap();
        let tmp = TempFile::new(&self.path, "workers.ts");

        let res = self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("workers")
                .arg(&cidl_path)
                .arg(tmp.path())
                .arg(&wrangler_path)
                .arg(domain)
                .current_dir("../../src/generator"),
        );

        match res {
            Ok(_) => Ok(self.read_temp_and_diff(&self.path, tmp).0),
            Err(err) => Err(diff_file(&self.path, err, true, self.check_only).0),
        }
    }

    pub fn generate_client(&self, cidl: &Path, domain: &str) -> Result<bool, bool> {
        let cidl_path = cidl.canonicalize().unwrap();
        let tmp = TempFile::new(&self.path, "client.ts");

        let res = self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("client")
                .arg(&cidl_path)
                .arg(tmp.path())
                .arg(domain)
                .current_dir("../../src/generator"),
        );

        match res {
            Ok(_) => Ok(self.read_temp_and_diff(&self.path, tmp).0),
            Err(err) => Err(diff_file(&self.path, err, true, self.check_only).0),
        }
    }

    /// Returns the error given by the command on failure
    fn run_command(&self, command: &mut Command) -> Result<(), String> {
        let output = command.output().expect("Failed to execute command");

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        return Ok(());
    }

    fn read_temp_and_diff(&self, path: &Path, tmp: TempFile) -> (bool, PathBuf) {
        let contents = fs::read(&tmp.path).expect("temp file to be readable");
        diff_file(
            path,
            String::from_utf8_lossy(&contents).to_string(),
            false,
            self.check_only,
        )
    }
}
