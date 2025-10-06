use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use similar::TextDiff;

pub enum DiffOpts {
    FailOnly,
    All,
}

// Compares unified file diffs, creating a `.new` snapshot file if a diff is found
///
/// Returns if a `.new` file created
fn diff_file(
    fixture_id: &String,
    out: OutputFile,
    new_contents: String,
    fail: bool,
    opt: &DiffOpts,
) -> (bool, PathBuf) {
    let name = if fail {
        format!("{}_{}_fail.out", fixture_id, out.base_name)
    } else {
        out.base_name.clone()
    };

    let new_path = out.path.with_file_name(format!("snap___{}", name));
    let old_path = out.path.with_file_name(name);

    // Empty if it doesn't even exist
    let old_contents = fs::read(&old_path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();

    let diff = TextDiff::from_lines(&old_contents, &new_contents);

    // No changes, write nothing
    if diff.ops().len() == 1 && matches!(diff.ops()[0].tag(), similar::DiffTag::Equal) {
        return (false, old_path);
    }

    let unified_diff = diff
        .unified_diff()
        .context_radius(3)
        .header(old_path.to_str().unwrap(), new_path.to_str().unwrap())
        .to_string();

    // Only print a diff if there is some dif.
    // If this is a run fail test, don't print file diff if there wasnt a failure
    if unified_diff.trim().is_empty() || matches!(opt, DiffOpts::FailOnly) && fail {
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

    fs::write(&new_path, new_contents).expect("path to be written");
    (true, new_path)
}

/// A temporary file for storing the outputs of a cloesce extractor/generator call
struct OutputFile {
    pub base_name: String,
    pub path: PathBuf,
}

impl OutputFile {
    fn new(fixture_path: &Path, base_name: &str) -> Self {
        if let Some(parent) = fixture_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent).unwrap();
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
}

impl Drop for OutputFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub struct Fixture {
    /// The path of a fixture entry point, ie a seed source file
    pub path: PathBuf,
    pub opt: DiffOpts,
    pub fixture_id: String,
}

impl Fixture {
    pub fn new(path: PathBuf, opt: DiffOpts) -> Self {
        let fixture_id = path.file_stem().unwrap().to_str().unwrap().to_owned();
        Self {
            fixture_id,
            path,
            opt,
        }
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
            Err(err) => Err(self.read_fail_and_diff(out, err)),
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
            Err(err) => Err(self.read_fail_and_diff(out, err)),
        }
    }

    pub fn generate_d1(&self, cidl: &Path) -> Result<(bool, PathBuf), (bool, PathBuf)> {
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
            Ok(_) => Ok(self.read_out_and_diff(out)),
            Err(err) => Err(self.read_fail_and_diff(out, err)),
        }
    }

    pub fn generate_workers(
        &self,
        cidl: &Path,
        wrangler: &Path,
        domain: &str,
    ) -> Result<(bool, PathBuf), (bool, PathBuf)> {
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
            Ok(_) => Ok(self.read_out_and_diff(out)),
            Err(err) => Err(self.read_fail_and_diff(out, err)),
        }
    }

    pub fn generate_client(
        &self,
        cidl: &Path,
        domain: &str,
    ) -> Result<(bool, PathBuf), (bool, PathBuf)> {
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
            Ok(_) => Ok(self.read_out_and_diff(out)),
            Err(err) => Err(self.read_fail_and_diff(out, err)),
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
            &self.fixture_id,
            out,
            String::from_utf8_lossy(&contents).to_string(),
            false,
            &self.opt,
        )
    }

    fn read_fail_and_diff(&self, out: OutputFile, err: String) -> (bool, PathBuf) {
        let normalized = err
            .find("==== CLOESCE ERROR ====")
            .map(|pos| err[pos..].to_string())
            .unwrap_or_else(|| err.to_string());

        diff_file(&self.fixture_id, out, normalized, true, &self.opt)
    }
}
