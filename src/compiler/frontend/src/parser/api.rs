use std::borrow::Cow;

use chumsky::prelude::*;

use ast::{CidlType, HttpVerb};

use crate::{
    ApiBlock, ApiBlockMethod, Span, Symbol, SymbolKind,
    lexer::Token,
    parser::{Extra, TokenInput, cidl_type},
};

struct PendingApiMethod<'src> {
    name: &'src str,
    span: Span,
    http_verb: HttpVerb,
    return_type: CidlType<'src>,
    parameters: Vec<PendingApiParam<'src>>,
}

enum PendingApiParam<'src> {
    SelfParam {
        data_source: Option<&'src str>,
    },
    Field {
        name: &'src str,
        span: Span,
        cidl_type: CidlType<'src>,
    },
}

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
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, ApiBlock<'src>, Extra<'tokens, 'src>> {
    // api Namespace { ... }
    just(Token::Api)
        .ignore_then(select! { Token::Ident(name) => name })
        .then(
            method()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|(namespace, pending_methods), e| {
            let methods = pending_methods
                .into_iter()
                .map(|m| map_method(m, namespace))
                .collect();

            ApiBlock {
                symbol: Symbol {
                    span: e.span(),
                    kind: SymbolKind::ApiDecl,
                    parent_name: Cow::Borrowed(namespace),
                    ..Default::default()
                },
                namespace,
                methods,
            }
        })
}

fn method<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, PendingApiMethod<'src>, Extra<'tokens, 'src>> {
    // http_verb methodName( ident* ) -> cidl_type
    http_verb()
        .then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
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
            |(((http_verb, (name, span)), parameters), return_type)| PendingApiMethod {
                name,
                span,
                http_verb,
                return_type,
                parameters,
            },
        )
}

fn parameter<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, PendingApiParam<'src>, Extra<'tokens, 'src>> {
    // [source DataSourceName]
    let source_tag = just(Token::LBracket)
        .ignore_then(just(Token::Source))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::RBracket));

    // self
    let self_parameter = source_tag
        .or_not()
        .then(select! { Token::Ident(name) if name == "self" => name })
        .map(|(data_source, _)| PendingApiParam::SelfParam { data_source });

    // ident: cidl_type
    let named_parameter = select! { Token::Ident(name) => name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map(|((name, span), cidl_type)| PendingApiParam::Field {
            name,
            span,
            cidl_type,
        });

    // [source DataSourceName] self | ident: cidl_type
    self_parameter.or(named_parameter)
}

fn http_verb<'tokens, 'src: 'tokens>()
-> impl Parser<'tokens, TokenInput<'tokens, 'src>, HttpVerb, Extra<'tokens, 'src>> {
    choice((
        just(Token::Get).map(|_| HttpVerb::Get),
        just(Token::Post).map(|_| HttpVerb::Post),
        just(Token::Put).map(|_| HttpVerb::Put),
        just(Token::Patch).map(|_| HttpVerb::Patch),
        just(Token::Delete).map(|_| HttpVerb::Delete),
    ))
}

fn map_method<'src>(method: PendingApiMethod<'src>, namespace: &'src str) -> ApiBlockMethod<'src> {
    let mut is_static = true;
    let mut data_source = None;
    let mut parameters = Vec::new();

    for parameter in method.parameters {
        match parameter {
            PendingApiParam::SelfParam {
                data_source: explicit_source,
            } => {
                is_static = false;
                if data_source.is_none() {
                    data_source = explicit_source;
                }
            }
            PendingApiParam::Field {
                name,
                span,
                cidl_type,
            } => {
                parameters.push(Symbol {
                    span,
                    name,
                    cidl_type,
                    kind: SymbolKind::ApiMethodParam,
                    parent_name: Cow::Owned(format!("{namespace}::{}", method.name)),
                });
            }
        }
    }

    ApiBlockMethod {
        symbol: Symbol {
            span: method.span,
            name: method.name,
            kind: SymbolKind::ApiMethodDecl,
            parent_name: Cow::Borrowed(namespace),
            ..Default::default()
        },
        is_static,
        data_source,
        http_verb: method.http_verb,
        return_type: method.return_type,
        parameters,
    }
}
