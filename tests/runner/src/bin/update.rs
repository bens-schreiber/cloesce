use clap::Parser;
use glob::glob;

use std::{fs, io};

#[derive(Parser)]
#[command(name = "update", version = "0.0.1")]
struct Cli {
    #[arg(short = 'd', long = "delete")]
    delete: bool,
}

/// Updates all `snap___X` to `X` in the fixtures dir
fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let pattern = "../fixtures/*/**";
    let fixtures = glob(pattern)
        .expect("valid glob pattern")
        .filter_map(Result::ok)
        .filter(|p| p.is_dir())
        .collect::<Vec<_>>();

    for fixture in fixtures {
        let patterns = vec![
            format!("{}/snap___*", fixture.display()),
            format!("{}/migrations/snap___*", fixture.display()),
        ];

        for pat in patterns {
            for entry in glob(&pat).expect("valid inner glob").filter_map(Result::ok) {
                if cli.delete {
                    fs::remove_file(entry).expect("remove file to work");
                    continue;
                }

                let file_name = entry.file_name().unwrap().to_string_lossy().to_string();
                let base_name = file_name.strip_prefix("snap___").expect("snap__ prefix");
                let base_path = entry.with_file_name(base_name);

                if base_path.exists() {
                    let new_contents = fs::read_to_string(&entry)?;
                    fs::write(&base_path, new_contents)?;
                    fs::remove_file(&entry)?;
                    println!(
                        "Updated snapshot file {} -> {}",
                        entry.display(),
                        base_path.display()
                    );
                    continue;
                }

                fs::rename(&entry, &base_path)?;
                println!(
                    "Created snapshot file {} -> {}",
                    entry.display(),
                    base_path.display()
                );
            }
        }
    }

    Ok(())
}
