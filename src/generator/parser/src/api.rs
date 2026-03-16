use chumsky::prelude::*;

use ast::{Api, ApiMethod, CidlType, CrudKind, Field, HttpVerb, MediaType, Symbol};
use lexer::Token;

use crate::{Extra, SymbolTable, sqlite_column_types};

struct PendingApiMethod {
    name: String,
    http_verb: HttpVerb,
    return_type: CidlType,
    parameters: Vec<PendingApiParam>,
}

enum PendingApiParam {
    SelfParam { data_source: Option<String> },
    Field { name: String, cidl_type: CidlType },
}

pub fn api_block<'t>() -> impl Parser<'t, &'t [Token], (Symbol, Api), Extra<'t>> {
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

    crud_tag
        .then_ignore(just(Token::Api))
        .then(select! { Token::Ident(name) => name })
        .then(
            method()
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(|((cruds, model_name), methods), e| {
            let symbol_table = e.state();
            let model_symbol = symbol_table.intern_global(&model_name);

            let methods = methods
                .into_iter()
                .map(|method| map_method(model_name.as_str(), method, symbol_table))
                .collect();

            (model_symbol, Api { cruds, methods })
        })
}

fn method<'t>() -> impl Parser<'t, &'t [Token], PendingApiMethod, Extra<'t>> {
    http_verb()
        .then(select! { Token::Ident(name) => name })
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
            |(((http_verb, name), parameters), return_type)| PendingApiMethod {
                name,
                http_verb,
                return_type,
                parameters,
            },
        )
}

fn parameter<'t>() -> impl Parser<'t, &'t [Token], PendingApiParam, Extra<'t>> {
    let source_tag = just(Token::At)
        .ignore_then(just(Token::Source))
        .ignore_then(just(Token::LParen))
        .ignore_then(select! { Token::Ident(name) => name })
        .then_ignore(just(Token::RParen));

    let self_parameter = source_tag
        .or_not()
        .then(select! { Token::Ident(name) if name == "self" => name })
        .map(|(data_source, _)| PendingApiParam::SelfParam { data_source });

    let named_parameter = select! { Token::Ident(name) => name }
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map(|(name, cidl_type)| PendingApiParam::Field { name, cidl_type });

    self_parameter.or(named_parameter)
}

fn cidl_type<'t>() -> impl Parser<'t, &'t [Token], CidlType, Extra<'t>> {
    recursive(|cidl_type| {
        let generic_wrapper = select! { Token::Ident(name) => name }
            .then_ignore(just(Token::LAngle))
            .then(cidl_type.clone())
            .then_ignore(just(Token::RAngle))
            .try_map(|(wrapper, inner), span| match wrapper.as_str() {
                "Option" => Ok(CidlType::nullable(inner)),
                "Result" => Ok(CidlType::http(inner)),
                "Array" => Ok(CidlType::array(inner)),
                "Paginated" => Ok(CidlType::paginated(inner)),
                "KvObject" => Ok(CidlType::KvObject(Box::new(inner))),
                "Partial" => match inner {
                    CidlType::Object(name) => Ok(CidlType::Partial(name)),
                    _ => Err(Rich::custom(span, "Partial<T> expects an object type")),
                },
                "DataSource" => match inner {
                    CidlType::Object(name) => Ok(CidlType::DataSource(name)),
                    _ => Err(Rich::custom(span, "DataSource<T> expects an object type")),
                },
                _ => Err(Rich::custom(span, "Unknown generic type wrapper")),
            });

        let primitive_keyword = choice((
            sqlite_column_types(),
            just(Token::Json).map(|_| CidlType::JsonValue),
            just(Token::Void).map(|_| CidlType::Void),
            just(Token::Blob).map(|_| CidlType::Blob),
            just(Token::Stream).map(|_| CidlType::Stream),
            just(Token::R2Object).map(|_| CidlType::R2Object),
        ));

        let object_type = select! { Token::Ident(name) => CidlType::Object(name) };

        choice((generic_wrapper, primitive_keyword, object_type)).boxed()
    })
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

fn map_method(
    model_name: &str,
    method: PendingApiMethod,
    symbol_table: &mut SymbolTable,
) -> ApiMethod {
    let method_scope = format!("api::{}", model_name);
    let param_scope = format!("api::{}::{}", model_name, method.name);

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
                    data_source = explicit_source
                        .map(|source_name| symbol_table.intern_scoped(model_name, &source_name));
                }
            }
            PendingApiParam::Field { name, cidl_type } => {
                parameters.push(Field {
                    symbol: symbol_table.intern_scoped(&param_scope, &name),
                    name,
                    cidl_type,
                });
            }
        }
    }

    ApiMethod {
        symbol: symbol_table.intern_scoped(&method_scope, &method.name),
        name: method.name,
        is_static,
        data_source,
        http_verb: method.http_verb,
        return_media: MediaType::default(),
        return_type: method.return_type,
        parameters_media: MediaType::default(),
        parameters,
    }
}
