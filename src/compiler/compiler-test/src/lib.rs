use std::path::PathBuf;

use ast::CloesceAst;
use frontend::{
    ParseAst,
    fmt::DisplayError,
    lexer::{CloesceLexer, LexSource},
    parser::CloesceParser,
};
use semantic::SemanticAnalysis;

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

/// Given a source string, lex and parse it into a [ParseAst], panicking if either step fails.
pub fn lex_and_parse(src: &str) -> ParseAst<'_> {
    let source = LexSource {
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

/// Given a source string, lex, parse, and semantically analyze it into a [CloesceAst],
/// panicking if any step fails.
pub fn src_to_ast(src: &str) -> CloesceAst<'_> {
    let parse = lex_and_parse(src);
    let (result, errors) = SemanticAnalysis::analyze(&parse);
    assert!(
        errors.is_empty(),
        "semantic analysis should succeed: {:#?}",
        errors
    );
    result
}
