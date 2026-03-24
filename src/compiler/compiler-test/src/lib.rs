use frontend::{ParseAst, lexer::CloesceLexer, parser::CloesceParser};

/// Given a source string, lex and parse it into a [ParseAst], panicking if either step fails.
pub fn lex_and_parse(src: &str) -> ParseAst {
    let tokens = CloesceLexer::default().lex(src).expect("lex to succeed");
    CloesceParser::default()
        .parse(tokens)
        .expect("parse to succeed")
}
