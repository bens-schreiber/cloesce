use std::{io::Write, panic, path::PathBuf};

use clap::{Parser, Subcommand, command};

use common::{
    CloesceAst,
    err::{GeneratorErrorKind, Result},
};
use workers::WorkersGenerator;
use wrangler::WranglerFormat;

#[derive(Parser)]
#[command(name = "generate", version = "0.0.3")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Validate {
        cidl_path: PathBuf,
    },
    Generate {
        #[command(subcommand)]
        target: GenerateTarget,
    },
}

#[derive(Subcommand)]
enum GenerateTarget {
    Wrangler {
        wrangler_path: PathBuf,
    },
    D1 {
        cidl_path: PathBuf,
        sqlite_path: PathBuf,
    },
    Workers {
        cidl_path: PathBuf,
        workers_path: PathBuf,
        wrangler_path: PathBuf,
        domain: String,
    },
    Client {
        cidl_path: PathBuf,
        client_path: PathBuf,
        domain: String,
    },
    All {
        cidl_path: PathBuf,
        wrangler_path: PathBuf,
        sqlite_path: PathBuf,
        workers_path: PathBuf,
        client_path: PathBuf,
        client_domain: String,
        workers_domain: String,
    },
}

fn main() {
    match panic::catch_unwind(run_cli) {
        Ok(Ok(())) => std::process::exit(0),
        Ok(Err(e)) if matches!(e.kind, GeneratorErrorKind::InvalidInputFile) => {
            eprintln!(
                "==== CLOESCE ERROR ====\nInvalid generator file input: {}\n",
                e.context
            );
        }
        Ok(Err(e)) => {
            eprintln!(
                r#"==== CLOESCE ERROR ====
Error [{:?}]: {}
Phase: {:?}
Context: {}
Suggested fix: {}"#,
                e.kind, e.description, e.phase, e.context, e.suggestion
            );
        }
        Err(e) => {
            eprintln!("==== GENERATOR PANIC CAUGHT ====");
            let msg = e
                .downcast_ref::<&str>()
                .copied()
                .or_else(|| e.downcast_ref::<String>().map(|s| s.as_str()))
                .unwrap_or("Panic occurred but couldn't extract info.");
            eprintln!("Panic info: {}", msg);
        }
    }

    std::process::exit(1);
}

fn run_cli() -> Result<()> {
    match Cli::parse().command {
        Commands::Validate { cidl_path } => {
            let cidl = CloesceAst::from_json(&cidl_path)?;
            cidl.validate_types()?;
            println!("Ok.");
        }
        Commands::Generate { target } => match target {
            GenerateTarget::Wrangler { wrangler_path } => generate_wrangler(&wrangler_path)?,
            GenerateTarget::D1 {
                cidl_path,
                sqlite_path,
            } => generate_d1(&cidl_path, &sqlite_path)?,
            GenerateTarget::Workers {
                cidl_path,
                workers_path,
                wrangler_path,
                domain,
            } => generate_workers(&cidl_path, &workers_path, &wrangler_path, &domain)?,
            GenerateTarget::Client {
                cidl_path,
                client_path,
                domain,
            } => generate_client(&cidl_path, &client_path, &domain)?,
            GenerateTarget::All {
                cidl_path,
                wrangler_path,
                sqlite_path,
                workers_path,
                client_path,
                client_domain,
                workers_domain,
            } => {
                let ast = CloesceAst::from_json(&cidl_path)?;
                ast.validate_types()?;
                println!("✅ Validation complete.");

                generate_wrangler(&wrangler_path)?;
                println!("✅ Wrangler generated.");

                generate_d1(&cidl_path, &sqlite_path)?;
                println!("✅ D1 schema generated.");

                generate_workers(&cidl_path, &workers_path, &wrangler_path, &workers_domain)?;
                println!("✅ Workers generated.");

                generate_client(&cidl_path, &client_path, &client_domain)?;
                println!("✅ Client generated.");

                println!("🎉 All generation steps completed successfully!");
            }
        },
    }

    Ok(())
}

fn generate_wrangler(wrangler_path: &PathBuf) -> Result<()> {
    let mut wrangler = WranglerFormat::from_path(wrangler_path);
    let mut spec = wrangler.as_spec();
    spec.generate_defaults();

    let wrangler_file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(wrangler_path)
        .map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;

    wrangler.update(spec, wrangler_file);
    Ok(())
}

fn generate_d1(cidl_path: &PathBuf, sqlite_path: &PathBuf) -> Result<()> {
    let mut sqlite_file = create_file_and_dir(sqlite_path)?;
    let ast = CloesceAst::from_json(cidl_path)?;
    ast.validate_types()?;

    let generated_sqlite = d1::generate_sql(&ast.models)?;
    sqlite_file
        .write_all(generated_sqlite.as_bytes())
        .expect("Could not write to file");
    Ok(())
}

fn generate_workers(
    cidl_path: &PathBuf,
    workers_path: &PathBuf,
    wrangler_path: &PathBuf,
    domain: &str,
) -> Result<()> {
    let ast = CloesceAst::from_json(cidl_path)?;
    ast.validate_types()?;

    let mut file = create_file_and_dir(workers_path)?;
    let wrangler = WranglerFormat::from_path(wrangler_path);

    let workers =
        WorkersGenerator::create(ast, wrangler.as_spec(), domain.to_string(), workers_path)?;
    file.write_all(workers.as_bytes())
        .expect("Could not write to file");
    Ok(())
}

fn generate_client(cidl_path: &PathBuf, client_path: &PathBuf, domain: &str) -> Result<()> {
    let ast = CloesceAst::from_json(cidl_path)?;
    ast.validate_types()?;

    let mut file = create_file_and_dir(client_path)?;
    file.write_all(client::generate_client_api(ast, domain.to_string()).as_bytes())
        .expect("Could not write to file");
    Ok(())
}

fn create_file_and_dir(path: &PathBuf) -> Result<std::fs::File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            GeneratorErrorKind::InvalidInputFile
                .to_error()
                .with_context(e.to_string())
        })?;
    }
    std::fs::File::create(path).map_err(|e| {
        GeneratorErrorKind::InvalidInputFile
            .to_error()
            .with_context(e.to_string())
    })
}
