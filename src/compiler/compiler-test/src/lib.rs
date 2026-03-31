use std::path::PathBuf;

use ast::CloesceAst;
use frontend::{
    ParseAst,
    lexer::{CloesceLexer, LexSource},
    parser::CloesceParser,
};

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

pub struct InMemorySource {
    pub src: String,
}

impl InMemorySource {
    pub fn new(src: impl Into<String>) -> Self {
        InMemorySource { src: src.into() }
    }
}

impl LexSource for InMemorySource {
    fn path(&self) -> PathBuf {
        PathBuf::from("<test>")
    }
    fn content(&self) -> std::io::Result<String> {
        Ok(self.src.clone())
    }
}

/// Given a source string, lex and parse it into a [ParseAst], panicking if either step fails.
pub fn lex_and_parse(src: &str) -> ParseAst {
    let tokens = match CloesceLexer.lex(InMemorySource::new(src)) {
        Ok(tokens) => tokens,
        Err(e) => {
            e.eprint();
            panic!("lexing should succeed")
        }
    };

    CloesceParser
        .parse(tokens.tokens)
        .expect("parse to succeed")
}

/// Given a source string, lex, parse, and semantically analyze it into a [CloesceAst],
/// panicking if any step fails.
pub fn src_to_ast(src: &str) -> CloesceAst {
    let parse = lex_and_parse(src);
    let (result, errors) = semantic::SemanticAnalysis::analyze(parse);
    assert!(
        errors.is_empty(),
        "semantic analysis should succeed: {:#?}",
        errors
    );
    result
}
