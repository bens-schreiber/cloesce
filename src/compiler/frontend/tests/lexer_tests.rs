use frontend::lexer::Token;
use logos::Logos;

#[test]
fn token_regexes_simple() {
    let cases = vec![
        ("model", Token::Model),
        ("env", Token::Env),
        ("int", Token::Int),
        ("double", Token::Double),
        ("{", Token::LBrace),
        ("}", Token::RBrace),
        ("(", Token::LParen),
        (")", Token::RParen),
        ("[", Token::LBracket),
        ("]", Token::RBracket),
        (":", Token::Colon),
        (",", Token::Comma),
        (".", Token::Dot),
        ("->", Token::Arrow),
        ("::", Token::DoubleColon),
        ("@", Token::At),
        ("\"hello\"", Token::StringLit("hello")),
        ("123", Token::IntLit(123)),
        ("1_000_000", Token::IntLit(1_000_000)),
        ("3.001", Token::DoubleLit(3.001)),
        ("_foo", Token::Ident("_foo")),
        ("bar123", Token::Ident("bar123")),
    ];

    for (input, expected) in cases {
        let mut lex = Token::lexer(input);
        let token = lex.next().unwrap();
        let expected_result = Ok(expected.clone());
        match (&token, &expected_result) {
            (Ok(Token::StringLit(a)), Ok(Token::StringLit(b))) => {
                assert_eq!(a, b, "input: {}", input)
            }
            (Ok(Token::IntLit(a)), Ok(Token::IntLit(b))) => {
                assert_eq!(a, b, "input: {}", input)
            }
            (Ok(Token::DoubleLit(a)), Ok(Token::DoubleLit(b))) => {
                assert!((a - b).abs() < 1e-8, "input: {}", input)
            }
            (Ok(Token::Ident(a)), Ok(Token::Ident(b))) => assert_eq!(a, b, "input: {}", input),
            _ => assert_eq!(token, expected_result, "input: {}", input),
        }
    }
}
