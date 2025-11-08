use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use similar::TextDiff;

// Compares unified file diffs, creating a `.new` snapshot file if a diff is found
///
/// Returns if a `.new` file created
fn diff_file(fixture_id: &String, out: OutputFile, new_contents: String, fail: bool) -> TestOutput {
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

type TestOutput = (bool, PathBuf);
type TestResult = Result<TestOutput, TestOutput>;

pub struct Fixture {
    /// The path of a fixture entry point, ie a seed source file
    pub path: PathBuf,
    pub fixture_id: String,
}

impl Fixture {
    pub fn new(path: PathBuf) -> Self {
        let fixture_id = path.file_stem().unwrap().to_str().unwrap().to_owned();
        Self { fixture_id, path }
    }

    pub fn extract_cidl(&self) -> TestResult {
        let out = OutputFile::new(&self.path, "cidl.pre.json");
        let res = self.run_command(
            Command::new("node")
                .arg("../../src/frontend/ts/dist/cli.js")
                .arg("extract")
                .arg("--in")
                .arg(&self.path)
                .arg("--out")
                .arg(out.path())
                .arg("--truncateSourcePaths"),
        );

        match res {
            Ok(_) => Ok(self.read_out_and_diff(out)),
            Err(err) => Err(self.read_fail_and_diff(out, err)),
        }
    }

    /// On all success, returns the cidl, else returns the failed file.
    pub fn generate_all(
        &self,
        pre_cidl: &Path,
        client_domain: &str,
        workers_domain: &str,
    ) -> TestResult {
        let pre_cidl_canon = pre_cidl.canonicalize().unwrap();

        let cidl_out = OutputFile::new(&self.path, "cidl.json");
        let wrangler_out = OutputFile::new(&self.path, "wrangler.toml");
        let workers_out = OutputFile::new(&self.path, "workers.ts");
        let client_out = OutputFile::new(&self.path, "client.ts");

        let cmd = self.run_command(
            Command::new("cargo")
                .arg("--quiet")
                .arg("run")
                .arg("generate")
                .arg(&pre_cidl_canon)
                .arg(cidl_out.path())
                .arg(wrangler_out.path())
                .arg(workers_out.path())
                .arg(client_out.path())
                .arg(client_domain)
                .arg(workers_domain)
                .current_dir("../../src/generator"),
        );

        let cidl_path = {
            match &cmd {
                Ok(_) => {
                    let (diff, path) = self.read_out_and_diff(cidl_out);
                    if diff {
                        return Ok((diff, path));
                    }

                    path
                }
                Err(err) => return Err(self.read_fail_and_diff(cidl_out, err.clone())),
            }
        };

        for out in [wrangler_out, workers_out, client_out] {
            match &cmd {
                Ok(_) => {
                    let (diff, path) = self.read_out_and_diff(out);
                    if diff {
                        return Ok((diff, path));
                    }
                }
                Err(err) => return Err(self.read_fail_and_diff(out, err.clone())),
            }
        }

        Ok((false, cidl_path))
    }

    pub fn migrate(&self, cidl: &Path) -> (TestResult, TestResult) {
        let migrated_cidl = OutputFile::new(
            &self.path.parent().unwrap().join("migrations/Initial.json"),
            "Initial.json",
        );
        let migrated_sql = OutputFile::new(
            &self.path.parent().unwrap().join("migrations/Initial.sql"),
            "Initial.sql",
        );

        let cidl_path = cidl.canonicalize().unwrap();

        let res = self.run_command(
            Command::new("cargo")
                .arg("--quiet")
                .arg("run")
                .arg("migrations")
                .arg(&cidl_path)
                .arg(migrated_cidl.path())
                .arg(migrated_sql.path())
                .current_dir("../../src/generator"),
        );

        let cidl_res = match &res {
            Ok(_) => Ok(self.read_out_and_diff(migrated_cidl)),
            Err(err) => Err(self.read_fail_and_diff(migrated_cidl, err.clone())),
        };

        let sql_res = match res {
            Ok(_) => Ok(self.read_out_and_diff(migrated_sql)),
            Err(err) => Err(self.read_fail_and_diff(migrated_sql, err)),
        };

        (cidl_res, sql_res)
    }

    /// Returns the error given by the command on failure
    fn run_command(&self, command: &mut Command) -> Result<(), String> {
        let output = command.output().expect("Failed to execute command");

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).to_string());
        }

        Ok(())
    }

    fn read_out_and_diff(&self, out: OutputFile) -> TestOutput {
        let contents = fs::read(&out.path).expect("temp file to be readable");
        diff_file(
            &self.fixture_id,
            out,
            String::from_utf8_lossy(&contents).to_string(),
            false,
        )
    }

    fn read_fail_and_diff(&self, out: OutputFile, err: String) -> TestOutput {
        let normalized = err
            .find("==== CLOESCE ERROR ====")
            .map(|pos| err[pos..].to_string())
            .unwrap_or_else(|| err.to_string());

        diff_file(&self.fixture_id, out, normalized, true)
    }
}
