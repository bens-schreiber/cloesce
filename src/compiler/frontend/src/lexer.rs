use ariadne::{Color, Label, Report};
use logos::Logos;

use std::fs;
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
    pub fn lex(&self, source: impl LexSource) -> Result<LexedFile, LexedFileError> {
        let path = source.path();
        let path_str = path.display().to_string();
        let src = match source.content() {
            Ok(s) => s,
            Err(e) => {
                let msg = format!("failed to read file: {}", e);
                let report = fail_file_report(&path_str, msg);
                return Err(LexedFileError {
                    reports: vec![report],
                    path_str,
                    src: String::new(),
                });
            }
        };

        let mut tokens = Vec::new();
        let mut spans: Vec<Range<usize>> = Vec::new();

        for (result, span) in Token::lexer(&src).spanned() {
            match result {
                Ok(token) => tokens.push((token, span)),
                Err(_) => spans.push(span),
            }
        }

        if spans.is_empty() {
            return Ok(LexedFile { tokens, path });
        }

        let reports = spans
            .into_iter()
            .map(|span| lex_error(&path_str, span))
            .collect();
        Err(LexedFileError {
            reports,
            path_str,
            src,
        })
    }

    pub fn lex_targets(
        &self,
        targets: Vec<PathBuf>,
    ) -> Result<Vec<LexedFile>, Vec<LexedFileError>> {
        let mut lexed_files = Vec::new();
        let mut errors = Vec::new();

        // TODO: should optimize by doing this in parallel, as well as stream the file
        // instead of reading it all into memory at once
        for path in targets {
            match self.lex(path) {
                Ok(res) => lexed_files.push(res),
                Err(e) => errors.push(e),
            }
        }

        if errors.is_empty() {
            Ok(lexed_files)
        } else {
            Err(errors)
        }
    }
}

pub trait LexSource {
    fn path(&self) -> PathBuf;
    fn content(&self) -> std::io::Result<String>;
}

impl LexSource for PathBuf {
    fn path(&self) -> PathBuf {
        self.clone()
    }
    fn content(&self) -> std::io::Result<String> {
        fs::read_to_string(self)
    }
}

pub type TokenRange = (Token, Range<usize>);
pub struct LexedFile {
    pub tokens: Vec<TokenRange>,
    pub path: PathBuf,
}

type LexReport = Report<'static, (String, Range<usize>)>;

pub struct LexedFileError {
    pub reports: Vec<LexReport>,
    pub path_str: String,
    pub src: String,
}

impl LexedFileError {
    pub fn eprint(&self) {
        let mut cache = ariadne::sources([(self.path_str.clone(), &self.src)]);
        for report in &self.reports {
            report.write(&mut cache, std::io::stderr()).ok();
        }
    }
}

fn fail_file_report(path_str: &String, msg: String) -> LexReport {
    Report::build(ariadne::ReportKind::Error, (path_str.clone(), 0..0))
        .with_message(msg)
        .finish()
}

fn lex_error(path_str: &String, span: Range<usize>) -> LexReport {
    Report::build(ariadne::ReportKind::Error, (path_str.clone(), span.clone()))
        .with_message("unexpected token")
        .with_label(
            Label::new((path_str.clone(), span))
                .with_message("not a valid token")
                .with_color(Color::Red),
        )
        .finish()
}
