use std::{io::Write, panic, path::PathBuf};

use clap::Parser;
use cli::open_file_or_create;
use frontend::{
    fmt::DisplayError,
    lexer::{CloesceLexer, LexTarget},
};
use tracing_subscriber::FmtSubscriber;

use codegen::{
    backend::BackendGenerator, client::ClientGenerator, wrangler::WranglerDefault,
    wrangler::WranglerGenerator,
};
use frontend::parser::CloesceParser;
use semantic::SemanticAnalysis;

#[derive(Parser)]
#[command(name = "compile")]
struct Args {
    cloesce_dir: PathBuf,
    wrangler_path: PathBuf,
    default_migrations_path: PathBuf,
    worker_url: String,

    #[arg(required = true, num_args = 1.., value_name = "PATH")]
    targets: Vec<PathBuf>,
}

fn main() {
    match panic::catch_unwind(compile) {
        Ok(Ok(())) => std::process::exit(0),
        Ok(Err(e)) => {
            tracing::error!("An error occurred during compilation: {e}");
            std::process::exit(1);
        }
        Err(e) => {
            tracing::error!("An uncaught error occurred during compilation: {:?}", e);
            std::process::abort();
        }
    }
}

fn compile() -> Result<(), String> {
    let subscriber = FmtSubscriber::builder().finish();
    tracing::subscriber::set_global_default(subscriber)
        .expect("Failed to set global default subscriber");

    let args = Args::parse();

    // Lexing
    let sources = args
        .targets
        .into_iter()
        .map(|p| {
            let src = std::fs::read_to_string(&p)
                .unwrap_or_else(|_| panic!("Failed to read source file: {}", p.display()));
            (src, p)
        })
        .collect::<Vec<(String, PathBuf)>>();

    let lexed = CloesceLexer::lex(sources.iter().map(|(src, path)| LexTarget {
        src: src.as_str(),
        path: path.clone(),
    }));
    if lexed.has_errors() {
        lexed.display_error(&lexed.file_table);
        return Err("lexing failed".to_string());
    }

    // Parsing
    let parse = CloesceParser::parse(&lexed.results, &lexed.file_table);
    if parse.has_errors() {
        parse.display_error(&lexed.file_table);
        return Err("parsing failed".to_string());
    }

    // Semantic
    let (ast, errors) = SemanticAnalysis::analyze(&parse.ast);
    if errors.len() > 0 {
        for error in &errors {
            error.display_error(&lexed.file_table);
        }
        return Err("semantic analysis failed".to_string());
    }

    // Codegen
    let wrangler = {
        let mut generator = WranglerGenerator::from_path(&args.wrangler_path);
        let mut spec = generator.as_spec();

        WranglerDefault::set_defaults(
            &mut spec,
            &ast,
            args.default_migrations_path.to_str().unwrap(),
        );
        generator.generate(spec)
    };

    let backend = BackendGenerator::generate(&ast);
    let client = ClientGenerator::generate(&ast, &args.worker_url);

    // Output CIDL
    {
        let cidl_path = args.cloesce_dir.join("cidl.json");
        let mut file = open_file_or_create(&cidl_path);
        file.write_all(ast.to_json().as_bytes())
            .expect("file to be written");
    };

    // Output Wrangler
    {
        let mut wrangler_file = open_file_or_create(&args.wrangler_path);
        wrangler_file
            .write_all(wrangler.as_bytes())
            .expect("file to be written");
    }

    // Output backend
    {
        let backend_path = args.cloesce_dir.join("backend.ts"); // TODO: hardcoded to ts
        let mut file = open_file_or_create(&backend_path);
        file.write_all(backend.as_bytes())
            .expect("file to be written");
    }

    // Output client
    {
        let client_path = args.cloesce_dir.join("client.ts"); // TODO: hardcoded to ts
        let mut file = open_file_or_create(&client_path);
        file.write_all(client.as_bytes())
            .expect("file to be written");
    }

    Ok(())
}
