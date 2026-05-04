use chumsky::prelude::*;

use ast::HttpVerb;

use crate::{
    ApiBlock, ApiBlockMethod, ApiBlockMethodParamKind, AstBlockKind, Symbol,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, cidl_type, kw, symbol, tags, typed_symbol},
};

/// Parses a block of the form:
///
/// ```cloesce
/// api Namespace {
///     http_verb methodName(ident1: cidl_type, ...) -> cidl_type
///
///     http_verb methodName(
///         [source MySource] self,
///         ident2: cidl_type,
///         ...
///     ) -> cidl_type
/// }
/// ```
pub fn api_block<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, AstBlockKind<'src>, Extra<'tokens, 'src>> {
    just(Token::Api)
        .ignore_then(symbol())
        .then(
            method()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(symbol, methods)| AstBlockKind::Api(ApiBlock { symbol, methods }))
}

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

    // [tag]* self
    let self_param = tags()
        .then_ignore(just(Token::SelfToken))
        .map_with(|tag_list, e| {
            ApiBlockMethodParamKind::SelfParam(Symbol {
                name: "self",
                span: e.span(),
                tags: tag_list,
                ..Default::default()
            })
        });

    // self | tagged_symbol: cidl_type
    let parameter = choice((
        self_param,
        typed_symbol().map(ApiBlockMethodParamKind::Param),
    ))
    .map_spanned(|p| p);

    // verb methodName(self, p1: type, ...) -> returnType { ... }
    verb.then(symbol())
        .then(
            parameter
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then_ignore(just(Token::Arrow))
        .then(cidl_type())
        .map_spanned(
            |(((http_verb, symbol), parameters), return_type)| ApiBlockMethod {
                symbol,
                http_verb,
                return_type,
                parameters,
            },
        )
}
