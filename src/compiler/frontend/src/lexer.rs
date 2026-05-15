//! Converts source strings into a stream of tokens, emitting file and span information for each token.
//!
//! # Overview
//!
//! The main entry point is [CloesceLexer::lex], which takes in a list of [LexTarget]s and produces
//! a [LexResult] containing the token stream for each file, along with any lexing errors and a [FileTable] for
//! resolving file IDs to source strings and paths for error reporting.
//!
//! Each [Token] is either some kind of punctuation, a literal, or an identifier. The only reserved keyword in Cloesce
//! is `self`. The `$` character is intentionally excluded from identifiers since it is used for generated content in the codegen phase.
//!
//! All comments are extracted and stored in a [CommentMap] such that the parser does not need to handle them.
//!
//! TODO: Currently, the lexer is synchronous across all files, but could be parallelized in the future.

use chumsky::span::{SimpleSpan, Spanned};
use logos::Logos;

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

#[derive(Logos, Debug, PartialEq, Clone)]
pub enum Token<'src> {
    // Reserved
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

pub type Span = SimpleSpan<usize, FileId>;
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

#[derive(Clone, Copy, Eq, PartialEq, Hash, Default)]
pub struct FileId(u16);

pub struct FileTable<'src> {
    table: HashMap<FileId, (&'src str, PathBuf)>,
}

impl<'src> FileTable<'src> {
    /// Panics if the ID is not found
    pub fn resolve(&self, file_id: FileId) -> (&str, &PathBuf) {
        let (src, path) = self.table.get(&file_id).expect("invalid file ID");
        (src, path)
    }

    pub fn cache(&self) -> impl ariadne::Cache<String> + '_ {
        ariadne::sources(
            self.table
                .values()
                .map(|(src, path)| (path.display().to_string(), *src)),
        )
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
            let file_id = FileId(id_seed.try_into().expect("too many files to lex"));

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
