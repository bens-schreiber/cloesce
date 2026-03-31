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
    #[regex(r#""[^"]*""#, |lex| { let s = lex.slice(); s[1..s.len()-1].to_string() })]
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

pub struct CloesceLexer;
impl CloesceLexer {
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
