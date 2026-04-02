use chumsky::span::{SimpleSpan, Spanned};
use logos::Logos;

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

use crate::{FileId, FileTable, Span};

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token<'src> {
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
    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        // Return a string slice without the quotes
        &s[1..s.len()-1]
    })]
    StringLit(&'src str),

    #[regex(r"[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<i64>().ok())]
    IntLit(i64),

    #[regex(r"[0-9][0-9_]*\.[0-9][0-9_]*", |lex| lex.slice().replace('_', "").parse::<f64>().ok())]
    DoubleLit(f64),

    // Identifiers (must come after keywords)
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice())]
    Ident(&'src str),

    // Skip whitespace and comments
    #[regex(r"[ \t\r\n]+", logos::skip)]
    #[regex(r"//[^\n]*", logos::skip, allow_greedy = true)]
    Error,
}

pub struct LexTarget<'src> {
    /// The full program source string.
    pub src: &'src str,

    /// The file path of the source, used for error reporting. Safe to be a dummy value for tests.
    pub path: PathBuf,
}

pub type SpannedToken<'src> = Spanned<Token<'src>, Span>;
pub struct LexedFile<'src> {
    /// The list of tokens produced by lexing the source string.
    /// Each token is annotated with it a file span, along with the file ID
    pub tokens: Vec<SpannedToken<'src>>,

    /// The file ID assigned to this source file, used for error reporting.
    /// Unique across all lexed files.
    pub file_id: FileId,
}

pub struct LexResult<'src> {
    pub results: Vec<LexedFile<'src>>,
    pub file_table: FileTable<'src>,
    pub errors: Vec<LexError>,
}
impl LexResult<'_> {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

pub struct LexError {
    pub error_spans: Vec<Range<usize>>,
    pub file_id: FileId,
}

pub struct CloesceLexer;
impl<'src> CloesceLexer {
    fn lex_file(src: &'src str, file_id: FileId) -> (Vec<SpannedToken<'src>>, Vec<LexError>) {
        let mut tokens = Vec::new();
        let mut error_spans: Vec<Range<usize>> = Vec::new();

        for (result, span) in Token::lexer(src).spanned() {
            match result {
                Ok(token) => tokens.push(Spanned {
                    inner: token,
                    span: SimpleSpan {
                        start: span.start,
                        end: span.end,
                        context: file_id,
                    },
                }),
                Err(_) => error_spans.push(span),
            }
        }

        let errors = error_spans
            .into_iter()
            .map(|span| LexError {
                error_spans: vec![span],
                file_id,
            })
            .collect();

        (tokens, errors)
    }

    pub fn lex(targets: impl IntoIterator<Item = LexTarget<'src>>) -> LexResult<'src> {
        let mut file_table = FileTable {
            table: HashMap::new(),
        };

        let mut results = Vec::new();
        let mut errors = Vec::new();

        // todo: could be parallelized
        for (id_seed, source) in targets.into_iter().enumerate() {
            let file_id = id_seed.try_into().expect("too many files to lex");

            file_table.table.insert(file_id, (source.src, source.path));

            let (tokens, errs) = Self::lex_file(source.src, file_id);
            results.push(LexedFile { tokens, file_id });
            errors.extend(errs);
        }

        LexResult {
            results,
            errors,
            file_table,
        }
    }
}
