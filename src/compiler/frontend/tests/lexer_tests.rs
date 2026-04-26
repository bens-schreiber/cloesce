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

    let mut int_lit = Token::lexer("42");
    assert_eq!(int_lit.next(), Some(Ok(Token::IntLit("42"))));

    let mut real_lit = Token::lexer("3.14");
    assert_eq!(real_lit.next(), Some(Ok(Token::RealLit("3.14"))));

    let mut regex_lit = Token::lexer("/[a-z]+/");
    assert_eq!(regex_lit.next(), Some(Ok(Token::RegexLit("[a-z]+"))));
}
