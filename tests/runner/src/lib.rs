use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use similar::TextDiff;

// Compares unified file diffs, creating a `.new` snapshot file if a diff is found
///
/// Returns if a `.new` file created
fn diff_file(
    out: OutputFile,
    new_contents: String,
    fail: bool,
    check_mode: bool,
) -> (bool, PathBuf) {
    let name = if fail {
        format!("{}_fail.out", out.base_name)
    } else {
        out.base_name.clone()
    };

    let new_path = out.path.with_file_name(format!("snap___{}", name));

    // Empty if it doesn't even exist
    let old_contents = fs::read(out.base_path())
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();

    let diff = TextDiff::from_lines(&old_contents, &new_contents);

    // No changes, write nothing
    if diff.ops().len() == 1 && matches!(diff.ops()[0].tag(), similar::DiffTag::Equal) {
        return (false, out.base_path());
    }

    let unified_diff = diff
        .unified_diff()
        .context_radius(3)
        .header(
            out.base_path().to_str().unwrap(),
            new_path.to_str().unwrap(),
        )
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

/// A temporary file for storing the outputs of a cloesce extractor/generator call
struct OutputFile {
    pub base_name: String,
    pub path: PathBuf,
}

impl OutputFile {
    fn new(fixture_path: &PathBuf, base_name: &str) -> Self {
        if !fixture_path.exists() {
            fs::create_dir_all(fixture_path).unwrap();
        }

        let path = fixture_path.with_file_name(format!("out.{base_name}"));

        std::fs::File::create(&path).unwrap();
        Self {
            path,
            base_name: base_name.to_string(),
        }
    }

    /// The full canonicalized path to the out file
    fn path(&self) -> PathBuf {
        self.path.canonicalize().unwrap()
    }

    /// The path to the non-temp version of this file which may or may not exist
    fn base_path(&self) -> PathBuf {
        self.path.with_file_name(&self.base_name)
    }
}

impl Drop for OutputFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub struct Fixture {
    /// The path of a fixture entry point, ie a seed source file
    pub path: PathBuf,

    pub check_only: bool,
}

impl Fixture {
    pub fn new(path: PathBuf, check_only: bool) -> Self {
        Self { path, check_only }
    }

    pub fn extract_cidl(&self) -> Result<(bool, PathBuf), (bool, PathBuf)> {
        let out = OutputFile::new(&self.path, "cidl.json");
        let res = self.run_command(
            Command::new("node")
                .arg("../../src/extractor/ts/dist/cli.js")
                .arg("--location")
                .arg(&self.path)
                .arg("--out")
                .arg(&out.path)
                .arg("--truncateSourcePaths"),
        );

        match res {
            Ok(_) => Ok(self.read_out_and_diff(out)),
            Err(err) => Err(diff_file(out, err, true, self.check_only)),
        }
    }

    pub fn generate_wrangler(&self) -> Result<(bool, PathBuf), (bool, PathBuf)> {
        let out = OutputFile::new(&self.path, "wrangler.toml");
        let res = self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("wrangler")
                .arg(out.path())
                .current_dir("../../src/generator"),
        );

        match res {
            Ok(_) => Ok(self.read_out_and_diff(out)),
            Err(err) => Err(diff_file(out, err, true, self.check_only)),
        }
    }

    pub fn generate_d1(&self, cidl: &Path) -> Result<bool, bool> {
        let cidl_path = cidl.canonicalize().unwrap();

        let out = OutputFile::new(
            &self
                .path
                .parent()
                .unwrap()
                .join("migrations/migrations.sql"),
            "migrations.sql",
        );

        let res = self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("d1")
                .arg(&cidl_path)
                .arg(out.path())
                .current_dir("../../src/generator"),
        );

        match res {
            Ok(_) => Ok(self.read_out_and_diff(out).0),
            Err(err) => Err(diff_file(out, err, true, self.check_only).0),
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
        let out = OutputFile::new(&self.path, "workers.ts");

        let res = self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("workers")
                .arg(&cidl_path)
                .arg(out.path())
                .arg(&wrangler_path)
                .arg(domain)
                .current_dir("../../src/generator"),
        );

        match res {
            Ok(_) => Ok(self.read_out_and_diff(out).0),
            Err(err) => Err(diff_file(out, err, true, self.check_only).0),
        }
    }

    pub fn generate_client(&self, cidl: &Path, domain: &str) -> Result<bool, bool> {
        let cidl_path = cidl.canonicalize().unwrap();
        let out = OutputFile::new(&self.path, "client.ts");

        let res = self.run_command(
            Command::new("cargo")
                .arg("run")
                .arg("generate")
                .arg("client")
                .arg(&cidl_path)
                .arg(out.path())
                .arg(domain)
                .current_dir("../../src/generator"),
        );
        match res {
            Ok(_) => Ok(self.read_out_and_diff(out).0),
            Err(err) => Err(diff_file(out, err, true, self.check_only).0),
        }
    }

    /// Returns the error given by the command on failure
    fn run_command(&self, command: &mut Command) -> Result<(), String> {
        let output = command.output().expect("Failed to execute command");

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        Ok(())
    }

    fn read_out_and_diff(&self, out: OutputFile) -> (bool, PathBuf) {
        let contents = fs::read(&out.path).expect("temp file to be readable");
        diff_file(
            out,
            String::from_utf8_lossy(&contents).to_string(),
            false,
            self.check_only,
        )
    }
}
