use chumsky::prelude::*;

use ast::{CidlType, CrudKind, HttpVerb};

use crate::{
    ApiBlock, ApiMethod, SpannedName, SpannedTypedName, UnresolvedName,
    lexer::Token,
    parser::{Extra, cidl_type},
};

struct PendingApiMethod {
    span_name: SpannedName,
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
        name_span: chumsky::span::SimpleSpan,
        cidl_type: CidlType,
    },
}

/// Parses a block of the form:
///
/// ```cloesce
/// @crud(get | save | list, ...)
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
    // @crud(get | save | list, ...)
    let crud_tag = just(Token::At)
        .ignore_then(just(Token::Crud))
        .ignore_then(just(Token::LParen))
        .ignore_then(
            crud_kind()
                .separated_by(just(Token::Comma))
                .at_least(1)
                .collect::<Vec<_>>(),
        )
        .then_ignore(just(Token::RParen))
        .or_not()
        .map(|cruds| cruds.unwrap_or_default());

    // api ApiName for ModelName { ... }
    crud_tag
        .then_ignore(just(Token::Api))
        .then(
            select! { Token::Ident(name) => name }.map_with(|name, e| SpannedName {
                name,
                span: e.span(),
            }),
        )
        .then(
            method()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((cruds, model_span_name), methods), e| {
            let methods = methods.into_iter().map(map_method).collect();

            ApiBlock {
                span: e.span(),
                model_name: UnresolvedName(model_span_name.name),
                cruds,
                methods,
            }
        })
}

fn method<'t>() -> impl Parser<'t, &'t [Token], PendingApiMethod, Extra<'t>> {
    // http_verb methodName(ident1: cidl_type, ...) -> cidl_type
    http_verb()
        .then(
            select! { Token::Ident(name) => name }.map_with(|name, e| SpannedName {
                name,
                span: e.span(),
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
            |(((http_verb, span_name), parameters), return_type)| PendingApiMethod {
                span_name,
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
        .map(|((name, name_span), cidl_type)| PendingApiParam::Field {
            name,
            name_span,
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

fn crud_kind<'t>() -> impl Parser<'t, &'t [Token], CrudKind, Extra<'t>> {
    choice((
        just(Token::Get).map(|_| CrudKind::GET),
        select! { Token::Ident(name) if name == "get" => CrudKind::GET },
        select! { Token::Ident(name) if name == "save" => CrudKind::SAVE },
        select! { Token::Ident(name) if name == "list" => CrudKind::LIST },
    ))
}

fn map_method(method: PendingApiMethod) -> ApiMethod {
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
                    data_source = explicit_source.map(UnresolvedName);
                }
            }
            PendingApiParam::Field {
                name,
                name_span,
                cidl_type,
            } => {
                parameters.push(SpannedTypedName {
                    span: name_span,
                    name,
                    ty: cidl_type,
                });
            }
        }
    }

    ApiMethod {
        span_name: method.span_name,
        is_static,
        data_source_name: data_source,
        http_verb: method.http_verb,
        return_type: method.return_type,
        parameters,
    }
}
