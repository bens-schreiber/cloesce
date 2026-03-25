use frontend::{
    ParseAst,
    lexer::CloesceLexer,
    parser::{CloesceParser, IdTable},
};
pub use semantic::SemanticResult;

/// Given a source string, lex and parse it into a [ParseAst], panicking if either step fails.
pub fn lex_and_parse(src: &str) -> ParseAst {
    let tokens = CloesceLexer::default().lex(src).expect("lex to succeed");
    let (ast, _) = CloesceParser::default()
        .parse(tokens)
        .expect("parse to succeed");
    ast
}

pub fn lex_and_parse_with_id(src: &str) -> (ParseAst, IdTable) {
    let tokens = CloesceLexer::default().lex(src).expect("lex to succeed");
    CloesceParser::default()
        .parse(tokens)
        .expect("parse to succeed")
}

pub fn src_to_ast(src: &str) -> SemanticResult {
    let tokens = CloesceLexer::default().lex(src).expect("lex to succeed");
    let (parse, _) = CloesceParser::default()
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
