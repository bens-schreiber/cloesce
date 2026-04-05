use ariadne::{Color, Label, Report};
use chumsky::error::RichReason;

use crate::FileTable;
use crate::lexer::{LexResult, Token};
use crate::parser::ParserResult;

impl std::fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Token::Env => write!(f, "env"),
            Token::Model => write!(f, "model"),
            Token::Source => write!(f, "source"),
            Token::For => write!(f, "for"),
            Token::Exposes => write!(f, "exposes"),
            Token::Service => write!(f, "service"),
            Token::Inject => write!(f, "inject"),
            Token::Api => write!(f, "api"),
            Token::Poo => write!(f, "poo"),
            Token::Sql => write!(f, "sql"),
            Token::Get => write!(f, "get"),
            Token::Post => write!(f, "post"),
            Token::Put => write!(f, "put"),
            Token::Patch => write!(f, "patch"),
            Token::Delete => write!(f, "delete"),
            Token::Include => write!(f, "include"),
            Token::Crud => write!(f, "crud"),
            Token::D1 => write!(f, "d1"),
            Token::R2 => write!(f, "r2"),
            Token::Kv => write!(f, "kv"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::LAngle => write!(f, "<"),
            Token::RAngle => write!(f, ">"),
            Token::Colon => write!(f, ":"),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
            Token::DoubleColon => write!(f, "::"),
            Token::StringLit(s) => write!(f, "\"{s}\""),
            Token::Ident(s) => write!(f, "{s}"),
            Token::Error => write!(f, "<error>"),
            Token::Nav => write!(f, "nav"),
            Token::Primary => write!(f, "primary"),
            Token::Foreign => write!(f, "foreign"),
            Token::Unique => write!(f, "unique"),
            Token::Use => write!(f, "use"),
            Token::Save => write!(f, "save"),
            Token::List => write!(f, "list"),
            Token::Keyfield => write!(f, "keyfield"),
            Token::Paginated => write!(f, "paginated"),
            Token::Vars => write!(f, "vars"),
            Token::Arrow => write!(f, "->"),
            Token::Optional => write!(f, "optional"),
            Token::SelfToken => write!(f, "self"),
        }
    }
}

pub trait DisplayError {
    fn display_error(&self, file_table: &FileTable);
}

impl DisplayError for LexResult<'_> {
    fn display_error(&self, file_table: &FileTable) {
        let mut cache = file_table.cache();

        for error in &self.errors {
            let (_, path) = file_table.resolve(error.file_id);
            let path_str = path.display().to_string();

            for span in &error.error_spans {
                let report =
                    Report::build(ariadne::ReportKind::Error, (path_str.clone(), span.clone()))
                        .with_message("unexpected token")
                        .with_label(
                            Label::new((path_str.clone(), span.clone()))
                                .with_message("not a valid token")
                                .with_color(Color::Red),
                        )
                        .finish();

                report.write(&mut cache, std::io::stderr()).ok();
            }
        }
    }
}

impl DisplayError for ParserResult<'_, '_> {
    fn display_error(&self, file_table: &FileTable) {
        let mut cache = file_table.cache();

        for error in &self.errors {
            let span = error.span();
            let file_id = span.context;
            let (_, path) = file_table.resolve(file_id);
            let path_str = path.display().to_string();
            let ariadne_span = span.start..span.end;

            let (message, label_msg) = match error.reason() {
                RichReason::ExpectedFound { expected, found } => {
                    let found_str = match found {
                        Some(tok) => format!("found '{}'", &**tok),
                        None => "found end of input".to_string(),
                    };
                    let expected_str = if expected.is_empty() {
                        "something else".to_string()
                    } else {
                        let parts: Vec<String> = expected.iter().map(|p| format!("{p}")).collect();
                        if parts.len() == 1 {
                            format!("expected {}", parts[0])
                        } else {
                            format!(
                                "expected one of {}",
                                parts[..parts.len() - 1].join(", ")
                                    + ", or "
                                    + &parts[parts.len() - 1]
                            )
                        }
                    };
                    (format!("{found_str}, {expected_str}"), found_str)
                }
                RichReason::Custom(msg) => (msg.clone(), msg.clone()),
            };

            let mut builder = Report::build(
                ariadne::ReportKind::Error,
                (path_str.clone(), ariadne_span.clone()),
            )
            .with_message(&message)
            .with_label(
                Label::new((path_str.clone(), ariadne_span.clone()))
                    .with_message(&label_msg)
                    .with_color(Color::Red),
            );

            for (ctx_pattern, ctx_span) in error.contexts() {
                builder = builder.with_label(
                    Label::new((path_str.clone(), ctx_span.start..ctx_span.end))
                        .with_message(format!("while parsing '{ctx_pattern}'"))
                        .with_color(Color::Yellow),
                );
            }

            builder.finish().write(&mut cache, std::io::stderr()).ok();
        }
    }
}
