use frontend::{ParseAst, lexer::CloesceLexer, parser::CloesceParser};
pub use semantic::SemanticResult;

/// Given a source string, lex and parse it into a [ParseAst], panicking if either step fails.
pub fn lex_and_parse(src: &str) -> ParseAst {
    let tokens = CloesceLexer::default().lex(src).expect("lex to succeed");
    CloesceParser::default()
        .parse(tokens)
        .expect("parse to succeed")
}

pub fn src_to_ast(src: &str) -> SemanticResult {
    let tokens = CloesceLexer::default().lex(src).expect("lex to succeed");
    let parse = CloesceParser::default()
        .parse(tokens)
        .expect("parse to succeed");
    let (result, errors) = semantic::SemanticAnalysis::analyze(parse);
    assert!(
        errors.is_empty(),
        "semantic analysis should succeed: {:#?}",
        errors
    );
    result
}
