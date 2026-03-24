use std::path::PathBuf;

use chumsky::prelude::*;

use ast::{CidlType, CrudKind, HttpVerb};

use crate::{
    ApiBlock, ApiBlockMethod, SpannedTypedName,
    lexer::Token,
    parser::{Extra, IdScope, It, cidl_type},
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
pub fn api_block<'t>(st: It) -> impl Parser<'t, &'t [Token], ApiBlock, Extra<'t>> {
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

    let st_methods = st.clone();
    let st_map = st.clone();

    // api ApiName for ModelName { ... }
    crud_tag
        .then_ignore(just(Token::Api))
        .then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::For))
        .then(select! { Token::Ident(name) => name })
        .then(
            method(st_methods)
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(
            move |(((cruds, api_name), model_name), pending_methods), e| {
                let id = st_map
                    .borrow_mut()
                    .intern(api_name.clone(), IdScope::Global);
                let model = st_map.borrow_mut().intern(model_name, IdScope::Global);
                let api_scope = IdScope::Api(api_name.clone());

                let methods = pending_methods
                    .into_iter()
                    .map(|m| map_method(m, &st_map, &api_scope))
                    .collect();

                ApiBlock {
                    id,
                    name: api_name,
                    span: e.span(),
                    file: PathBuf::new(),
                    model,
                    cruds,
                    methods,
                }
            },
        )
}

fn method<'t>(st: It) -> impl Parser<'t, &'t [Token], PendingApiMethod, Extra<'t>> {
    let st_param = st.clone();

    // http_verb methodName(ident1: cidl_type, ...) -> cidl_type
    http_verb()
        .then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then(
            parameter(st_param)
                .separated_by(just(Token::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        )
        .then_ignore(just(Token::Arrow))
        .then(cidl_type(st))
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

fn parameter<'t>(st: It) -> impl Parser<'t, &'t [Token], PendingApiParam, Extra<'t>> {
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
        .then(cidl_type(st))
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

fn map_method(method: PendingApiMethod, st: &It, api_scope: &IdScope) -> ApiBlockMethod {
    let id = st.borrow_mut().intern(method.name, api_scope.clone());

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
                    data_source =
                        explicit_source.map(|s| st.borrow_mut().intern(s, IdScope::Global));
                }
            }
            PendingApiParam::Field {
                name,
                name_span,
                cidl_type,
            } => {
                let id = st.borrow_mut().intern(name.clone(), api_scope.clone());
                parameters.push(SpannedTypedName {
                    id,
                    span: name_span,
                    name,
                    cidl_type,
                });
            }
        }
    }

    ApiBlockMethod {
        id,
        span: method.span,
        is_static,
        data_source_name: data_source,
        http_verb: method.http_verb,
        return_type: method.return_type,
        parameters,
    }
}
