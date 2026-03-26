use chumsky::prelude::*;

use ast::{CidlType, HttpVerb};

use crate::{
    ApiBlock, ApiBlockMethod, FileSpan, Symbol, SymbolKind,
    lexer::Token,
    parser::{Extra, cidl_type},
};

struct PendingApiMethod {
    name: String,
    span: SimpleSpan,
    http_verb: HttpVerb,
    return_type: CidlType,
    parameters: Vec<PendingApiParam>,
}

enum PendingApiParam {
    SelfParam {
        data_source: Option<String>,
    },
    Field {
        name: String,
        span: SimpleSpan,
        cidl_type: CidlType,
    },
}

/// Parses a block of the form:
///
/// ```cloesce
/// api ApiName for ModelName {
///     http_verb methodName(ident1: cidl_type, ...) -> cidl_type
///
///     http_verb methodName(
///         @source(DataSourceName) self,
///         ident2: cidl_type,
///         ...
///     ) -> cidl_type
/// }
/// ```
pub fn api_block<'t>() -> impl Parser<'t, &'t [Token], ApiBlock, Extra<'t>> {
    // api ApiName for ModelName { ... }
    just(Token::Api)
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::For))
        .then(select! { Token::Ident(name) => name })
        .then(
            method()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((name, model), pending_methods), e| {
            let methods = pending_methods
                .into_iter()
                .map(|m| map_method(m, &name))
                .collect();

            ApiBlock {
                symbol: Symbol {
                    span: FileSpan::from_simple_span(e.span()),
                    name,
                    kind: SymbolKind::ApiDecl,
                    parent_name: model.clone(),
                    ..Default::default()
                },
                model,
                methods,
            }
        })
}

fn method<'t>() -> impl Parser<'t, &'t [Token], PendingApiMethod, Extra<'t>> {
    // http_verb methodName(ident1: cidl_type, ...) -> cidl_type
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

fn parameter<'t>() -> impl Parser<'t, &'t [Token], PendingApiParam, Extra<'t>> {
    // @source(DataSourceName) self
    let source_tag = just(Token::At)
        .ignore_then(just(Token::Source))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::RParen));

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

    // @source(DataSourceName) self | ident: cidl_type
    self_parameter.or(named_parameter)
}

fn http_verb<'t>() -> impl Parser<'t, &'t [Token], HttpVerb, Extra<'t>> {
    choice((
        just(Token::Get).map(|_| HttpVerb::Get),
        just(Token::Post).map(|_| HttpVerb::Post),
        just(Token::Put).map(|_| HttpVerb::Put),
        just(Token::Patch).map(|_| HttpVerb::Patch),
        just(Token::Delete).map(|_| HttpVerb::Delete),
    ))
}

fn map_method(method: PendingApiMethod, api_name: &str) -> ApiBlockMethod {
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
                    span: FileSpan::from_simple_span(span),
                    name,
                    cidl_type,
                    kind: SymbolKind::ApiMethodParam,
                    parent_name: format!("{api_name}::{}", method.name),
                    ..Default::default()
                });
            }
        }
    }

    ApiBlockMethod {
        symbol: Symbol {
            span: FileSpan::from_simple_span(method.span),
            name: method.name,
            kind: SymbolKind::ApiMethodDecl,
            parent_name: api_name.to_string(),
            ..Default::default()
        },
        is_static,
        data_source,
        http_verb: method.http_verb,
        return_type: method.return_type,
        parameters,
    }
}
