use chumsky::prelude::*;

use ast::HttpVerb;

use crate::{
    ApiBlock, ApiBlockMethod, ApiBlockMethodParamKind, AstBlockKind, Symbol,
    lexer::Token,
    parser::{Extra, TokenInput, cidl_type},
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
        .ignore_then(select! { Token::Ident(name) => name })
        .then(
            method()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|(namespace, methods), e| {
            AstBlockKind::Api(ApiBlock {
                symbol: Symbol {
                    span: e.span(),
                    ..Default::default()
                },
                namespace,
                methods,
            })
        })
}

fn method<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ApiBlockMethod<'src>, Extra<'tokens, 'src>> {
    http_verb()
        .then(
            select! { Token::Ident(name) => name }.map_with(|name, e| Symbol {
                span: e.span(),
                name,
                ..Default::default()
            }),
        )
        .then(
            parameter()
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then_ignore(just(Token::Arrow))
        .then(cidl_type())
        .map(
            |(((http_verb, symbol), parameters), return_type)| ApiBlockMethod {
                symbol,
                http_verb,
                return_type,
                parameters,
            },
        )
}

fn parameter<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ApiBlockMethodParamKind<'src>, Extra<'tokens, 'src>>
{
    // [source DataSourceName]
    let source_tag = just(Token::LBracket)
        .ignore_then(just(Token::Source))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::RBracket));

    // [source DataSourceName] self
    let self_parameter = source_tag
        .or_not()
        .then(just(Token::SelfToken).map_with(|_, e| e.span()))
        .map(|(data_source, span)| ApiBlockMethodParamKind::SelfParam {
            symbol: Symbol {
                span,
                ..Default::default()
            },
            data_source,
        });

    // ident: cidl_type
    let named_parameter = select! { Token::Ident(name) => name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map(|((name, span), ty)| {
            ApiBlockMethodParamKind::Field(Symbol {
                name,
                span,
                cidl_type: ty,
                ..Default::default()
            })
        });

    self_parameter.or(named_parameter)
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
