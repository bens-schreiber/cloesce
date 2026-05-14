use std::path::PathBuf;

use frontend::{
    Ast,
    err::DisplayError,
    lexer::{CloesceLexer, LexTarget},
    parser::CloesceParser,
};
use idl::CloesceIdl;
use semantic::SemanticAnalysis;

mod src_str;
pub use src_str::*;

/// Compares two strings disregarding tabs, amount of spaces, and amount of newlines.
/// Ensures that some expr is present in another expr.
#[macro_export]
macro_rules! expected_str {
    ($got:expr, $expected:expr) => {{
        let clean = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert!(
            clean(&$got.to_string()).contains(&clean(&$expected.to_string())),
            "Expected:\n`{}`\n\ngot:\n`{}`",
            $expected,
            $got
        );
    }};
}

/// Given a source string, lex and parse it into an [Ast], panicking if either step fails.
pub fn lex_and_ast(src: &str) -> Ast<'_> {
    let source = LexTarget {
        src,
        path: PathBuf::from("<test>"),
    };

    let lexed = CloesceLexer::lex(vec![source]);
    if lexed.has_errors() {
        lexed.display_error(&lexed.file_table);
        panic!("lexing should succeed");
    }

    let result = CloesceParser::parse(&lexed.results, &lexed.file_table);
    if result.has_errors() {
        result.display_error(&lexed.file_table);
        panic!("parse should succeed");
    }

    result.ast
}

/// Given a source string, lex, parse, and semantically analyze it into a [CloesceIdl],
/// panicking if any step fails.
pub fn src_to_idl(src: &str) -> CloesceIdl<'_> {
    let source = LexTarget {
        src,
        path: PathBuf::from("<test>"),
    };

    let lexed = CloesceLexer::lex(vec![source]);
    if lexed.has_errors() {
        lexed.display_error(&lexed.file_table);
        panic!("lexing should succeed");
    }

    let result = CloesceParser::parse(&lexed.results, &lexed.file_table);
    if result.has_errors() {
        result.display_error(&lexed.file_table);
        panic!("parse should succeed");
    }

    let (result, errors) = SemanticAnalysis::analyze(&result.ast);
    if !errors.is_empty() {
        for error in &errors {
            error.display_error(&lexed.file_table);
        }
        panic!("semantic analysis should succeed");
    }

    result
}
