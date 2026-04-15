use chumsky::prelude::*;

use ast::{CidlType, HttpVerb};

use crate::{
    ApiBlock, ApiBlockMethod, ApiBlockMethodParamKind, AstBlockKind, Symbol,
    lexer::Token,
    parser::{Extra, MapSpanned, TokenInput, cidl_type, symbol, typed_symbol},
};

/// Parses a block of the form:
///
/// ```cloesce
/// api Namespace {
///     http_verb methodName(ident1: cidl_type, ...) -> cidl_type
///
///     http_verb methodName(
///         [source MySource]
///         self,
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

fn method<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, crate::Spd<ApiBlockMethod<'src>>, Extra<'tokens, 'src>>
{
    http_verb()
        .then(symbol())
        .then(
            parameter()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then_ignore(just(Token::Arrow))
        .then(cidl_type())
        .map_spanned(|(((http_verb, symbol), parameters), return_type)| ApiBlockMethod {
            symbol,
            http_verb,
            return_type,
            parameters,
        })
}

fn parameter<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, crate::Spd<ApiBlockMethodParamKind<'src>>, Extra<'tokens, 'src>>
{
    // [source DataSourceName]
    let source_tag = just(Token::LBracket)
        .ignore_then(just(Token::Source))
        .ignore_then(symbol())
        .then_ignore(just(Token::RBracket));

    // [source DataSourceName] self
    let self_parameter = source_tag
        .or_not()
        .then(just(Token::SelfToken).map_with(|_, e| e.span()))
        .map_spanned(|(data_source, span)| ApiBlockMethodParamKind::SelfParam {
            symbol: Symbol {
                span,
                name: "self",
                cidl_type: CidlType::default(),
            },
            data_source,
        });

    self_parameter.or(typed_symbol().map_spanned(ApiBlockMethodParamKind::Field))
}

fn http_verb<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, HttpVerb, Extra<'tokens, 'src>> {
    select! {
        Token::Ident("get") => HttpVerb::Get,
        Token::Ident("post") => HttpVerb::Post,
        Token::Ident("put") => HttpVerb::Put,
        Token::Ident("patch") => HttpVerb::Patch,
        Token::Ident("delete") => HttpVerb::Delete,
    }
}
