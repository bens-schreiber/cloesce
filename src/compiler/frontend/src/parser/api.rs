use chumsky::prelude::*;

use idl::HttpVerb;

use crate::{
    ApiBlock, ApiBlockMethod, AstBlockKind, MethodSource, Symbol,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, cidl_type, kw, method_body, symbol},
};

/// ```cloesce
/// api Namespace {
///     self(SourceName) http_verb methodName -> cidl_type {
///         [tag]* ident: cidl_type
///
///         inject {
///             ident1
///             ident2::target(arg)
///             ident3::{ target1(arg1), target2(arg2) }
///         }
///     }
/// }
/// ```
pub fn api_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    kw!(Api)
        .ignore_then(symbol())
        .then(
            method()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, methods)| AstBlockKind::Api(ApiBlock { symbol, methods }))
        .boxed()
}

/// ```cloesce
/// self(sourceName) verb methodName -> returnType {
///     [tag]* param: cidl_type
///
///     inject {
///        ident1
///        ident2::target(arg)
///        ident3::{ target1(arg1), target2(arg2) }
///     }
/// }
/// ```
fn method<'tokens, 'src: 'tokens>() -> impl Parser<
    'tokens,
    TokenInput<'tokens, 'src>,
    crate::Spd<ApiBlockMethod<'src>>,
    Extra<'tokens, 'src>,
> {
    let verb = choice((
        kw!(Get).to(HttpVerb::Get),
        kw!(Post).to(HttpVerb::Post),
        kw!(Put).to(HttpVerb::Put),
        kw!(Patch).to(HttpVerb::Patch),
        kw!(Delete).to(HttpVerb::Delete),
    ));

    // `self` | `self(SourceName)`
    let source = kw!(SelfKw)
        .ignore_then(
            symbol()
                .delimited_by(just(Token::LParen), just(Token::RParen))
                .or_not(),
        )
        .map_spanned(|source| MethodSource { source });

    source
        .or_not()
        .then(verb)
        .then(symbol())
        .then(just(Token::Arrow).ignore_then(cidl_type()).or_not())
        .then(method_body())
        .map_spanned(
            |((((source, http_verb), symbol), return_type), (parameters, injects))| {
                ApiBlockMethod {
                    symbol: Symbol {
                        cidl_type: return_type.unwrap_or_default(),
                        ..symbol
                    },
                    source,
                    http_verb,
                    parameters,
                    injects,
                }
            },
        )
        .boxed()
}
