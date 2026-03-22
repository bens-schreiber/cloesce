use logos::Logos;

use std::ops::Range;
use std::path::PathBuf;

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token {
    // Keywords
    #[token("env")]
    Env,
    #[token("model")]
    Model,
    #[token("source")]
    Source,
    #[token("for")]
    For,
    #[token("exposes")]
    Exposes,
    #[token("service")]
    Service,
    #[token("inject")]
    Inject,
    #[token("api")]
    Api,
    #[token("poo")]
    Poo,
    #[token("sql")]
    Sql,
    #[token("get")]
    Get,
    #[token("post")]
    Post,
    #[token("put")]
    Put,
    #[token("patch")]
    Patch,
    #[token("delete")]
    Delete,
    #[token("include")]
    Include,
    #[token("crud")]
    Crud,

    // Environment binding types
    #[token("d1")]
    D1,
    #[token("r2")]
    R2,
    #[token("kv")]
    Kv,

    // Primitive types
    #[token("string")]
    String,
    #[token("int")]
    Int,
    #[token("double")]
    Double,
    #[token("date")]
    Date,
    #[token("json")]
    Json,
    #[token("bool")]
    Bool,
    #[token("void")]
    Void,
    #[token("blob")]
    Blob,
    #[token("stream")]
    Stream,
    #[token("R2Object")]
    R2Object,

    // Punctuation
    #[token("{")]
    LBrace,
    #[token("}")]
    RBrace,
    #[token("(")]
    LParen,
    #[token(")")]
    RParen,
    #[token("[")]
    LBracket,
    #[token("]")]
    RBracket,
    #[token("<")]
    LAngle,
    #[token(">")]
    RAngle,
    #[token(":")]
    Colon,
    #[token(",")]
    Comma,
    #[token(".")]
    Dot,
    #[token("->")]
    Arrow,
    #[token("::")]
    DoubleColon,
    #[token("@")]
    At,

    // Literals
    #[regex(r#""[^"]*""#, |lex| lex.slice().to_string())]
    StringLit(String),

    #[regex(r"[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<i64>().ok())]
    IntLit(i64),

    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<f64>().ok())]
    DoubleLit(f64),

    // Identifiers (must come after keywords)
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Ident(String),

    // Skip whitespace and comments
    #[regex(r"[ \t\r\n]+", logos::skip)]
    #[regex(r"//[^\n]*", logos::skip, allow_greedy = true)]
    Error,
}

#[derive(Default)]
pub struct Lexer;
impl Lexer {
    pub fn lex(&self, source: &str) -> Result<Vec<(Token, Range<usize>)>, Vec<String>> {
        let mut tokens = Vec::new();
        let mut errors = Vec::new();

        for (result, span) in Token::lexer(source).spanned() {
            match result {
                Ok(token) => tokens.push((token, span)),
                Err(_) => errors.push(format!("{}:{:?}: unexpected token", source, span)),
            }
        }

        if errors.is_empty() {
            Ok(tokens)
        } else {
            Err(errors)
        }
    }

    pub fn lex_targets(
        &self,
        targets: Vec<PathBuf>,
    ) -> Result<Vec<(Token, Range<usize>)>, Vec<String>> {
        let mut tokens = Vec::new();
        let mut errors = Vec::new();

        for target in targets {
            match self.lex(&std::fs::read_to_string(&target).unwrap_or_else(|e| {
                errors.push(format!("Failed to read {:?}: {}", target, e));
                String::new()
            })) {
                Ok(new_tokens) => tokens.extend(new_tokens),
                Err(new_errors) => errors.extend(new_errors),
            }
        }

        if errors.is_empty() {
            Ok(tokens)
        } else {
            Err(errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regexes() {
        // Arrange
        let lexer = Lexer::default();
        let source = r#"model Foo {
    [primary id]
    id: int

    @default("hello world")
    name: string

    @default(0)
    field: int

    @default(3.14)
    field2: double

    // comment should be ignored
}
"#;
        // Act
        let result = lexer.lex(source);

        // Assert
        assert!(result.is_ok());

        let tokens = result.unwrap();
        assert_eq!(
            tokens,
            [
                (Token::Model, 0..5),
                (Token::Ident("Foo".into()), 6..9),
                (Token::LBrace, 10..11),
                (Token::LBracket, 16..17),
                (Token::Ident("primary".into()), 17..24),
                (Token::Ident("id".into()), 25..27),
                (Token::RBracket, 27..28),
                (Token::Ident("id".into()), 33..35),
                (Token::Colon, 35..36),
                (Token::Ident("int".into()), 37..40),
                (Token::LBracket, 46..47),
                (Token::Ident("default".into()), 47..54),
                (Token::StringLit("\"hello world\"".into()), 55..68),
                (Token::RBracket, 68..69),
                (Token::Ident("name".into()), 74..78),
                (Token::Colon, 78..79),
                (Token::Ident("string".into()), 80..86),
                (Token::LBracket, 92..93),
                (Token::Ident("default".into()), 93..100),
                (Token::IntLit(0), 101..102),
                (Token::RBracket, 102..103),
                (Token::Ident("field".into()), 108..113),
                (Token::Colon, 113..114),
                (Token::Ident("int".into()), 115..118),
                (Token::LBracket, 124..125),
                (Token::Ident("default".into()), 125..132),
                (Token::DoubleLit(3.14), 133..137),
                (Token::RBracket, 137..138),
                (Token::Ident("field2".into()), 143..149),
                (Token::Colon, 149..150),
                (Token::Ident("double".into()), 151..157),
                (Token::RBrace, 192..193)
            ]
        );
    }
}
