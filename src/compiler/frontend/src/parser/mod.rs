mod api;
mod data_source;
mod env;
mod model;

use ast::CidlType;
use chumsky::extra;
use chumsky::input::MappedInput;
use chumsky::prelude::*;

use crate::AstBlockKind;
use crate::FileTable;
use crate::Span;
use crate::Symbol;
use crate::ValidatorLiteral;
use crate::ValidatorTag;
use crate::lexer::LexedFile;
use crate::lexer::SpannedToken;
use crate::lexer::Token;
use crate::{InjectBlock, ParseAst, PlainOldObjectBlock, ServiceBlock, Spd};

type TokenInput<'tokens, 'src> =
    MappedInput<'tokens, Token<'src>, Span, &'tokens [SpannedToken<'src>]>;

type Extra<'tokens, 'src> = extra::Err<Rich<'tokens, Token<'src>, Span>>;

trait MapSpanned<'tokens, 'src: 'tokens, O>:
    Parser<'tokens, TokenInput<'tokens, 'src>, O, Extra<'tokens, 'src>> + Sized
{
    fn map_spanned<F, T>(
        self,
        f: F,
    ) -> impl Parser<'tokens, TokenInput<'tokens, 'src>, Spd<T>, Extra<'tokens, 'src>>
    where
        F: Fn(O) -> T,
    {
        self.try_map(move |out, span: Span| {
            Ok(Spd {
                block: f(out),
                span,
            })
        })
    }
}

impl<'tokens, 'src: 'tokens, P, O> MapSpanned<'tokens, 'src, O> for P where
    P: Parser<'tokens, TokenInput<'tokens, 'src>, O, Extra<'tokens, 'src>> + Sized
{
}

pub struct ParserResult<'src, 'tokens> {
    pub ast: ParseAst<'src>,
    pub errors: Vec<Rich<'tokens, Token<'src>, Span>>,
}

impl ParserResult<'_, '_> {
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

pub struct CloesceParser;
impl CloesceParser {
    pub fn parse<'tokens, 'src: 'tokens>(
        lexed: &'tokens [LexedFile<'src>],
        file_table: &'tokens FileTable<'src>,
    ) -> ParserResult<'src, 'tokens> {
        let mut ast = ParseAst::default();
        let mut errors = Vec::new();

        for lf in lexed {
            let (src, _) = file_table.resolve(lf.file_id);

            let input = lf.tokens.split_spanned(Span {
                start: 0,
                end: src.len(),
                context: lf.file_id,
            });

            let res = parser().parse(input).into_result();

            match res {
                Ok(res) => ast.merge(res),
                Err(errs) => errors.extend(errs),
            }
        }

        ParserResult { ast, errors }
    }
}

fn parser<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ParseAst<'src>, Extra<'tokens, 'src>> {
    choice((
        model::model_block(),
        data_source::data_source_block(),
        env::env_block().map_spanned(|b| b),
        api::api_block().map_spanned(|b| b),
        service_block().map_spanned(|b| b),
        poo_block().map_spanned(|b| b),
        inject_block().map_spanned(|b| b),
    ))
    .repeated()
    .collect::<Vec<_>>()
    .map(|blocks| ParseAst { blocks })
}

/// Parses a block of the form:
///
/// ```cloesce
/// poo MyObject {
///     ident1: cidl_type
///     ident2: cidl_type
///     ...
/// }
/// ```
fn poo_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    just(Token::Poo)
        .ignore_then(symbol())
        .then(
            typed_symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, fields)| {
            AstBlockKind::PlainOldObject(PlainOldObjectBlock { symbol, fields })
        })
}

/// Parses a block of the form:
///
/// ```cloesce
/// inject {
///     ident1
///     ident2
///     ...
/// }
/// ```
fn inject_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    just(Token::Inject)
        .ignore_then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|symbols| AstBlockKind::Inject(InjectBlock { symbols }))
}

/// Parses a block of the form:
///
/// ```cloesce
/// service MyAppService {
///     ident1: InjectedService
///     ident2: cidl_type
/// }
/// ```
fn service_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    // ident: cidl_type
    // NOTE: Does not capture validator tags.
    let attribute = symbol()
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map_with(|(sym, cidl_type), e| Symbol {
            span: Span::new(sym.span.context(), sym.span.start..e.span().end),
            cidl_type,
            ..sym
        });

    // service ServiceName { ... }
    just(Token::Service)
        .ignore_then(symbol())
        .then(
            attribute
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, fields)| AstBlockKind::Service(ServiceBlock { symbol, fields }))
}

/// Parses an identifier and captures its name + span info into a `Symbol`.
///
/// Does not capture cidl type / validator (sets to [CidlType::default()])
fn symbol<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Symbol<'src>, Extra<'tokens, 'src>> {
    select! { Token::Ident(name) => name }.map_with(|name, e| Symbol {
        span: e.span(),
        name,
        ..Default::default()
    })
}

fn validator_tag<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Spd<ValidatorTag<'src>>, Extra<'tokens, 'src>> {
    let literal = choice((
        select! { Token::RealLit(s) => ValidatorLiteral::Real(s) },
        select! { Token::IntLit(s) => ValidatorLiteral::Int(s) },
        select! { Token::StringLit(s) => ValidatorLiteral::Str(s) },
        select! { Token::RegexLit(s) => ValidatorLiteral::Regex(s) },
    ));

    just(Token::LBracket)
        .ignore_then(select! { Token::Ident(name) => name })
        .then(literal.repeated().collect::<Vec<_>>())
        .then_ignore(just(Token::RBracket))
        .map_spanned(|(name, args)| ValidatorTag { name, args })
}

/// Parses a block of the form:
/// ```cloesce
/// [tag args...]
/// ident: cidl_type
/// ```
fn typed_symbol<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Symbol<'src>, Extra<'tokens, 'src>> {
    validator_tag()
        .repeated()
        .collect::<Vec<_>>()
        .then(symbol())
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map_with(|((validator_tags, sym), cidl_type), e| Symbol {
            span: Span::new(sym.span.context(), sym.span.start..e.span().end),
            cidl_type,
            tags: validator_tags,
            ..sym
        })
        // Without this box, Apple `ld` linker breaks
        // (a symbol name over 1.2 million characters is generated, exceeding the name limit)
        .boxed()
}

fn cidl_type<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, CidlType<'src>, Extra<'tokens, 'src>> {
    recursive(|cidl_type| {
        let wrapper = select! { Token::Ident(name) => name }
            .then_ignore(just(Token::LAngle))
            .then(cidl_type.clone())
            .then_ignore(just(Token::RAngle))
            .try_map(|(wrapper, inner), span| match wrapper {
                "Option" => Ok(CidlType::nullable(inner)),
                "Array" => Ok(CidlType::array(inner)),
                "Paginated" => Ok(CidlType::paginated(inner)),
                "KvObject" => Ok(CidlType::KvObject(Box::new(inner))),
                "Partial" => match inner {
                    CidlType::UnresolvedReference { name } => {
                        Ok(CidlType::Partial { object_name: name })
                    }
                    _ => Err(Rich::custom(span, "Partial<T> expects an object type")),
                },
                "DataSource" => match inner {
                    CidlType::UnresolvedReference { name: model_name } => {
                        Ok(CidlType::DataSource { model_name })
                    }
                    _ => Err(Rich::custom(span, "DataSource<T> expects an object type")),
                },
                _ => Err(Rich::custom(span, "Unknown generic type wrapper")),
            });

        let primitive_keyword = choice((
            just(Token::Ident("string")).to(CidlType::String),
            just(Token::Ident("int")).to(CidlType::Int),
            just(Token::Ident("uint")).to(CidlType::Uint),
            just(Token::Ident("real")).to(CidlType::Real),
            just(Token::Ident("date")).to(CidlType::DateIso),
            just(Token::Ident("bool")).to(CidlType::Boolean),
            just(Token::Ident("json")).to(CidlType::Json),
            just(Token::Ident("void")).to(CidlType::Void),
            just(Token::Ident("blob")).to(CidlType::Blob),
            just(Token::Ident("stream")).to(CidlType::Stream),
            just(Token::Ident("R2Object")).to(CidlType::R2Object),
            just(Token::Env).to(CidlType::Env),
        ));

        let unresolved_type = select! { Token::Ident(name) => name }
            .map(|name: &str| CidlType::UnresolvedReference { name });

        choice((wrapper, primitive_keyword, unresolved_type)).boxed()
    })
}
