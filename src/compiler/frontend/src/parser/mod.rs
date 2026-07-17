mod api;
mod data_source;
mod env;
mod model;

use chumsky::extra;
use chumsky::input::MappedInput;
use chumsky::prelude::*;
use idl::{CidlType, CrudKind};

use crate::lexer::{FileTable, LexedFile, SpannedToken, Token};
use crate::{
    ArgumentLiteral, Ast, AstBlockKind, InjectBlock, InjectEntry, Keyword, PlainOldObjectBlock,
    Span, Spd, Symbol, Tag,
};

pub type ParserError<'tokens, 'src> = Vec<Rich<'tokens, Token<'src>, Span>>;

/// Parses a list of [LexedFile]s into an [Ast], returning any errors encountered during parsing.
pub fn parse<'tokens, 'src: 'tokens>(
    lexed: &'tokens [LexedFile<'src>],
    file_table: &'tokens FileTable<'src>,
) -> Result<Ast<'src>, ParserError<'tokens, 'src>> {
    let mut ast = Ast::default();
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

    errors.is_empty().then_some(ast).ok_or(errors)
}

fn parser<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Ast<'src>, Extra<'tokens, 'src>> {
    choice((
        model::model_block(),
        data_source::data_source_block(),
        env::d1_binding_block().map_spanned(|b| b),
        env::kv_binding_block().map_spanned(|b| b),
        env::r2_binding_block().map_spanned(|b| b),
        env::durable_binding_block().map_spanned(|b| b),
        env::vars_block().map_spanned(|b| b),
        api::api_block().map_spanned(|b| b),
        poo_block().map_spanned(|b| b),
        inject_block().map_spanned(|b| b),
    ))
    .repeated()
    .collect::<Vec<_>>()
    .map(|blocks| Ast { blocks })
}

/// ```cloesce
/// poo MyObject {
///     ident1: cidl_type
///     ident2: cidl_type
///     ...
/// }
/// ```
fn poo_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    kw!(Poo)
        .ignore_then(symbol())
        .then(
            tagged_typed_symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, fields)| {
            AstBlockKind::PlainOldObject(PlainOldObjectBlock { symbol, fields })
        })
        .boxed()
}

/// ```cloesce
/// inject {
///     ident1
///     ident2
///     ...
/// }
/// ```
fn inject_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    kw!(Inject)
        .ignore_then(
            symbol()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|symbols| AstBlockKind::Inject(InjectBlock { symbols }))
        .boxed()
}

/// Parses any number of `[ ... ]` tags, returning them as a vector of spanned [Tag]s.
fn tags<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Vec<Spd<Tag<'src>>>, Extra<'tokens, 'src>> {
    // [validator arg]
    let validator = just(Token::LBracket)
        .ignore_then(choice((
            kw!(GreaterThan).to(Keyword::GreaterThan),
            kw!(GreaterThanOrEqual).to(Keyword::GreaterThanOrEqual),
            kw!(LessThanOrEqual).to(Keyword::LessThanOrEqual),
            kw!(LessThan).to(Keyword::LessThan),
            kw!(Step).to(Keyword::Step),
            kw!(Len).to(Keyword::Len),
            kw!(MinLen).to(Keyword::MinLen),
            kw!(MaxLen).to(Keyword::MaxLen),
            kw!(Regex).to(Keyword::Regex),
        )))
        .then(choice((
            select! { Token::RealLit(s) => ArgumentLiteral::Real(s) },
            select! { Token::IntLit(s) => ArgumentLiteral::Int(s) },
            select! { Token::StringLit(s) => ArgumentLiteral::Str(s) },
            select! { Token::RegexLit(s) => ArgumentLiteral::Regex(s) },
        )))
        .then_ignore(just(Token::RBracket))
        .map(|(name, argument)| Tag::Validator { name, argument });

    // [crud get|list|save, get|list|save, ...]
    let crud_tag = just(Token::LBracket)
        .then(kw!(Crud))
        .ignore_then(
            choice((
                kw!(Get).to(CrudKind::Get),
                kw!(List).to(CrudKind::List),
                kw!(Save).to(CrudKind::Save),
            ))
            .map_spanned(|b| b)
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map(|kinds| Tag::Crud { kinds });

    // [inject binding1, DurableDo(arg1, arg2), GlobalDo(), ...]
    let inject_entry = symbol()
        .then(
            symbol()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen))
                .or_not(),
        )
        .map(|(symbol, args)| match args {
            Some(args) => InjectEntry::Context { symbol, args },
            None => InjectEntry::Binding(symbol),
        });

    let inject_tag = just(Token::LBracket)
        .then(kw!(Inject))
        .ignore_then(
            inject_entry
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RBracket))
        .map(|entries| Tag::Inject { entries });

    // [internal]
    let internal_tag = just(Token::LBracket)
        .then(kw!(Internal))
        .then_ignore(just(Token::RBracket))
        .map(|_| Tag::Internal);

    // [instance]
    let instance_tag = just(Token::LBracket)
        .then(kw!(Instance))
        .then_ignore(just(Token::RBracket))
        .map(|_| Tag::Instance);

    // [source SourceName]
    let source_tag = just(Token::LBracket)
        .then(kw!(Source))
        .ignore_then(select! { Token::Ident(name) => name }.map_spanned(|name| name))
        .then_ignore(just(Token::RBracket))
        .map(|name| Tag::Source { name });

    choice((
        validator,
        crud_tag,
        inject_tag,
        internal_tag,
        instance_tag,
        source_tag,
    ))
    .map_spanned(|tag| tag)
    .repeated()
    .collect::<Vec<_>>()
    .boxed()
}

///```cloesce
/// ident
/// ```
///
/// NOTE: Does not include tags.
fn symbol<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Symbol<'src>, Extra<'tokens, 'src>> {
    select! { Token::Ident(name) => name }.map_with(|name, e| Symbol {
        span: e.span(),
        name,
        ..Default::default()
    })
}

/// ```cloesce
/// ident: cidl_type
/// ```
fn typed_symbol<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Symbol<'src>, Extra<'tokens, 'src>> {
    symbol()
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map_with(|(sym, cidl_type), e| Symbol {
            span: Span::new(sym.span.context(), sym.span.start..e.span().end),
            cidl_type,
            ..sym
        })
        .boxed()
}

/// ```cloesce
/// [tags]
/// ident: cidl_type
/// ```
fn tagged_typed_symbol<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, Symbol<'src>, Extra<'tokens, 'src>> {
    tags()
        .then(typed_symbol())
        .map(|(tags, sym)| Symbol { tags, ..sym })
        .boxed()
}

fn cidl_type<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, CidlType<'src>, Extra<'tokens, 'src>> {
    let primitive_keyword = choice((
        kw!(TString).to(CidlType::String),
        kw!(TInt).to(CidlType::Int),
        kw!(TReal).to(CidlType::Real),
        kw!(TDate).to(CidlType::DateIso),
        kw!(TBool).to(CidlType::Boolean),
        kw!(TJson).to(CidlType::Json),
        kw!(TBlob).to(CidlType::Blob),
        kw!(TStream).to(CidlType::Stream),
        kw!(TR2Object).to(CidlType::R2Object),
    ));

    recursive(|cidl_type| {
        let generic = choice((
            kw!(GOption).to(Keyword::GOption),
            kw!(GArray).to(Keyword::GArray),
            kw!(GKvObject).to(Keyword::GKvObject),
            kw!(GPartial).to(Keyword::GPartial),
        ))
        .then(
            cidl_type
                .clone()
                .delimited_by(just(Token::LAngle), just(Token::RAngle)),
        )
        .try_map(|(wrapper, inner), span| match wrapper {
            Keyword::GOption => Ok(CidlType::nullable(inner)),
            Keyword::GArray => Ok(CidlType::array(inner)),
            Keyword::GKvObject => Ok(CidlType::KvObject(Box::new(inner))),
            Keyword::GPartial => match inner {
                CidlType::Object { name } => Ok(CidlType::Partial { object_name: name }),
                _ => Err(Rich::custom(span, "Partial<T> expects an object type")),
            },
            _ => unreachable!(
                "All generic wrapper keywords should be covered in the match arms above"
            ),
        });

        // If unresolved, assume its an object
        let unresolved_type =
            select! { Token::Ident(name) => name }.map(|name: &str| CidlType::Object { name });

        choice((generic, primitive_keyword, unresolved_type)).boxed()
    })
}

/// Converts a [Keyword] to a `just` [Token] parser
macro_rules! kw {
    ($kw:ident) => {
        just(Token::from(crate::Keyword::$kw))
    };
}
pub(crate) use kw;

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
                inner: f(out),
                span,
            })
        })
    }
}

impl<'tokens, 'src: 'tokens, P, O> MapSpanned<'tokens, 'src, O> for P where
    P: Parser<'tokens, TokenInput<'tokens, 'src>, O, Extra<'tokens, 'src>> + Sized
{
}
