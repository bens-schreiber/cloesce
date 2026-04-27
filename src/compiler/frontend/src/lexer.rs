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
    #[token("self")]
    SelfToken,

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
    #[token("::")]
    DoubleColon,
    #[token("->")]
    Arrow,

    // Literals
    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        &s[1..s.len()-1]
    })]
    StringLit(&'src str),

    #[regex(r"[0-9]+\.[0-9]+", |lex| lex.slice())]
    RealLit(&'src str),

    #[regex(r"[0-9]+", |lex| lex.slice())]
    IntLit(&'src str),

    #[regex(r"/[^/\n][^/\n]*/", |lex| {
        let s = lex.slice();
        &s[1..s.len()-1]
    })]
    RegexLit(&'src str),

    // Identifiers (must come after keywords)
    // NOTE: `$` is intentionally excluded since it is used for generated content
    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice())]
    Ident(&'src str),

    // Comments
    #[regex(r"//[^\n]*", |lex| lex.slice(), allow_greedy = true)]
    Comment(&'src str),

    // Skip whitespace
    #[regex(r"[ \t\r\n]+", logos::skip)]
    Error,
}

pub struct LexTarget<'src> {
    /// The full program source string.
    pub src: &'src str,

    /// The file path of the source, used for error reporting. Safe to be a dummy value for tests.
    pub path: PathBuf,
}

pub type SpannedToken<'src> = Spanned<Token<'src>, Span>;

pub struct CommentMap<'src> {
    /// Each entry is `(start_byte_offset, comment_text)` including the `//` prefix.
    pub entries: Vec<(usize, &'src str)>,
}

impl<'src> CommentMap<'src> {
    /// Return all comments whose start offset is in `[prev_end, node_start)`.
    pub fn between(&self, prev_end: usize, node_start: usize) -> &[(usize, &'src str)] {
        let lo = self.entries.partition_point(|(off, _)| *off < prev_end);
        let hi = self.entries.partition_point(|(off, _)| *off < node_start);
        &self.entries[lo..hi]
    }
}

pub struct LexedFile<'src> {
    /// The list of tokens produced by lexing the source string.
    /// Each token is annotated with it a file span, along with the file ID.
    ///
    /// Comment tokens have been removed; see `comment_map`.
    pub tokens: Vec<SpannedToken<'src>>,

    /// The file ID assigned to this source file, used for error reporting.
    /// Unique across all lexed files.
    pub file_id: FileId,

    /// All comments found in the source, ordered by byte offset.
    pub comment_map: CommentMap<'src>,
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
    fn lex_file(
        src: &'src str,
        file_id: FileId,
    ) -> (Vec<SpannedToken<'src>>, CommentMap<'src>, Vec<LexError>) {
        let mut tokens = Vec::new();
        let mut comments = Vec::new();
        let mut error_spans: Vec<Range<usize>> = Vec::new();

        for (result, span) in Token::lexer(src).spanned() {
            match result {
                Ok(Token::Comment(text)) => {
                    comments.push((span.start, text));
                }
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

        (tokens, CommentMap { entries: comments }, errors)
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

            let (tokens, comment_map, errs) = Self::lex_file(source.src, file_id);
            results.push(LexedFile {
                tokens,
                file_id,
                comment_map,
            });
            errors.extend(errs);
        }

        LexResult {
            results,
            errors,
            file_table,
        }
    }
}
