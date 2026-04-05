use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use similar::TextDiff;

// Compares unified file diffs, creating a `.new` snapshot file if a diff is found
///
/// Returns if a `.new` file created
fn diff_file(out: OutputFile, new_contents: String) -> (bool, PathBuf) {
    let name = out.base_name.clone();

    let new_path = out.path.with_file_name(format!("snap__{}", name));
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

struct OutputFile {
    pub base_name: String,
    pub path: PathBuf,
}

impl OutputFile {
    fn new(dir: &Path, base_name: &str) -> Self {
        if !dir.exists() {
            fs::create_dir_all(dir).unwrap();
        }

        let path = dir.join(format!("out.{base_name}"));
        if !path.exists() {
            std::fs::File::create(&path).expect("file to have been created");
        }

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
        if self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| !name.starts_with("out."))
        {
            return;
        }

        let _ = fs::remove_file(&self.path);
    }
}

pub struct Fixture {
    /// The path of a fixture entry point, ie a seed source file
    pub path: PathBuf,
    pub fixture_id: String,
}

impl Fixture {
    pub fn new(path: PathBuf) -> Self {
        let fixture_id = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap()
            .to_owned();
        Self { fixture_id, path }
    }

    fn get_project_root(&self) -> PathBuf {
        self.path
            .parent()
            .and_then(|p| p.parent()) // fixtures
            .and_then(|p| p.parent()) // e2e
            .and_then(|p| p.parent()) // tests
            .and_then(|p| p.parent()) // project root
            .expect("Failed to calculate project root")
            .to_path_buf()
    }

    /// On all success, returns the cidl and wrangler config, otherwise returns the failed file.
    pub fn compile(&self, workers_domain: &str) -> Result<(bool, PathBuf, PathBuf), String> {
        let project_root = self.get_project_root();
        let compiler_dir = project_root.join("src/compiler");

        let fixture_dir = self.path.parent().unwrap();
        let cidl_out = OutputFile::new(fixture_dir, "cidl.json");
        let wrangler_out = OutputFile::new(fixture_dir, "wrangler.toml");
        let backend_out = OutputFile::new(fixture_dir, "backend.ts");
        let client_out = OutputFile::new(fixture_dir, "client.ts");

        let cloesce_dir = &self.path.parent().unwrap();
        let default_migrations_path = "migrations";
        let cloesce_source = fixture_dir.join("schema.cloesce");

        tracing::info!("Generating outputs for fixture {}", self.fixture_id);
        let cmd = self.run_command(
            Command::new("./target/release/compile")
                .arg(cloesce_dir)
                .arg(wrangler_out.path())
                .arg(default_migrations_path)
                .arg(workers_domain)
                .arg(cloesce_source)
                .arg("--snap")
                .current_dir(&compiler_dir),
        );

        let mut has_diff = false;

        let cidl_path = {
            match cmd {
                Ok(_) => {
                    let (diff, path) = self.read_out_and_diff(cidl_out);
                    has_diff |= diff;

                    path
                }
                Err(err) => return Err(err),
            }
        };

        let wrangler_path = {
            match cmd {
                Ok(_) => {
                    let (diff, path) = self.read_out_and_diff(wrangler_out);
                    has_diff |= diff;

                    path
                }
                Err(err) => return Err(err),
            }
        };

        for out in [backend_out, client_out] {
            match cmd {
                Ok(_) => {
                    let (diff, _) = self.read_out_and_diff(out);
                    has_diff |= diff;
                }
                Err(err) => return Err(err),
            }
        }

        Ok((has_diff, cidl_path, wrangler_path))
    }

    pub fn migrate(&self, cidl: &Path, wrangler_path: &Path) -> Result<(bool, bool), String> {
        let fixture_root = self.path.parent().expect("fixture root to exist");
        let cidl_path = cidl.canonicalize().unwrap();
        let compiler_dir = {
            let project_root = self.get_project_root();
            project_root.join("src/compiler")
        };

        tracing::info!("Migrating CIDL for fixture {}", self.fixture_id);
        let res = self.run_command(
            Command::new("./target/release/migrate")
                .arg(&cidl_path)
                .arg("--fixed")
                .arg("--all")
                .arg("out.Initial")
                .arg(&wrangler_path)
                .arg(fixture_root)
                .current_dir(&compiler_dir),
        );

        let res = match res {
            Ok(res) => res,
            Err(err) => return Err(err.clone()),
        };

        let mut bindings = Vec::<String>::new();
        for line in res.lines() {
            if let Some(start_idx) = line.find("Finished migration for binding '") {
                let rest = &line[start_idx + "Finished migration for binding '".len()..];
                if let Some(end_idx) = rest.find("'.") {
                    let binding = rest[..end_idx].to_string();
                    if !bindings.contains(&binding) {
                        bindings.push(binding);
                    }
                }
            }
        }

        if bindings.is_empty() {
            tracing::info!(
                "No migrations were run for fixture {}, skipping diffing",
                self.fixture_id
            );
            return Ok((false, false));
        }

        let mut sql_changed = false;
        let mut cidl_changed = false;
        for binding in bindings {
            let fixture_path = fixture_root.join(format!("migrations/{binding}"));
            let cidl_out = OutputFile::new(&fixture_path, "Initial.json");
            let sql_out = OutputFile::new(&fixture_path, "Initial.sql");

            sql_changed |= self.read_out_and_diff(sql_out).0;
            cidl_changed |= self.read_out_and_diff(cidl_out).0;
        }

        Ok((sql_changed, cidl_changed))
    }

    /// Returns the error given by the command on failure
    fn run_command(&self, command: &mut Command) -> Result<String, String> {
        let output = command.output().expect("Failed to execute command");
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        if !output.status.success() {
            let msg = if !stderr.trim().is_empty() {
                stderr
            } else {
                stdout
            };
            return Err(msg);
        }

        if stdout.trim().is_empty() {
            return Ok(stderr);
        }

        if stderr.trim().is_empty() {
            return Ok(stdout);
        }

        Ok(format!("{stdout}\n{stderr}"))
    }

    fn read_out_and_diff(&self, out: OutputFile) -> (bool, PathBuf) {
        let contents = fs::read(&out.path).expect("temp file to be readable");
        diff_file(out, String::from_utf8_lossy(&contents).to_string())
    }
}
