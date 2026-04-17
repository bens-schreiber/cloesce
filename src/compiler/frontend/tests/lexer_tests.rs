use frontend::lexer::Token;
use logos::Logos;

#[test]
fn token_regexes() {
    let mut str_lit = Token::lexer("\"hello\"");
    assert_eq!(str_lit.next(), Some(Ok(Token::StringLit("hello"))));

    let mut ident_lit = Token::lexer("my_var123");
    assert_eq!(ident_lit.next(), Some(Ok(Token::Ident("my_var123"))));

    let mut comment_lit = Token::lexer("// this is a comment");
    assert_eq!(
        comment_lit.next(),
        Some(Ok(Token::Comment("// this is a comment")))
    );
}
