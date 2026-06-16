//! Utilities for testing the compiler.

use std::path::PathBuf;

use frontend::{Ast, err::DisplayError, lexer, lexer::LexTarget, parser};
use idl::CloesceIdl;
use semantic::{self};

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

    let (lexed, file_table) = match lexer::lex(vec![source]) {
        Ok((results, file_table)) => (results, file_table),
        Err((errors, file_table)) => {
            errors.display_error(&file_table);
            panic!("lexing should succeed");
        }
    };

    match parser::parse(&lexed, &file_table) {
        Ok(ast) => ast,
        Err(errors) => {
            errors.display_error(&file_table);
            panic!("parse should succeed");
        }
    }
}

/// Given a source string, lex, parse, and semantically analyze it into a [CloesceIdl],
/// panicking if any step fails.
pub fn src_to_idl(src: &str) -> CloesceIdl<'_> {
    let source = LexTarget {
        src,
        path: PathBuf::from("<test>"),
    };

    let (lexed, file_table) = match lexer::lex(vec![source]) {
        Ok((results, file_table)) => (results, file_table),
        Err((errors, file_table)) => {
            errors.display_error(&file_table);
            panic!("lexing should succeed");
        }
    };

    let parsed = match parser::parse(&lexed, &file_table) {
        Ok(ast) => ast,
        Err(errors) => {
            errors.display_error(&file_table);
            panic!("parse should succeed");
        }
    };

    match semantic::analyze(&parsed) {
        Ok(idl) => idl,
        Err(errors) => {
            for error in &errors {
                error.display_error(&file_table);
            }
            panic!("semantic analysis should succeed");
        }
    }
}
