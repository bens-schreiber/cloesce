use std::ops::Range;

use ast::CidlType;
use chumsky::extra;
use chumsky::prelude::*;

use crate::SpannedName;
use crate::SpannedTypedName;
use crate::lexer::Token;
use crate::{
    ApiBlock, DataSourceBlock, ModelBlock, ParseAst, PlainOldObjectBlock, ServiceBlock,
    UnresolvedName, WranglerEnvBlock,
};

pub(crate) type Extra<'t> = extra::Err<Rich<'t, Token>>;

mod api;
mod data_source;
mod env;
mod model;
mod service;

#[derive(Default)]
pub struct CloesceParser;

impl CloesceParser {
    pub fn parse(
        &self,
        tokens: Vec<(Token, Range<usize>)>,
    ) -> Result<ParseAst, Vec<Rich<'static, Token>>> {
        let tokens = tokens.into_iter().map(|(t, _)| t).collect::<Vec<_>>();

        self._parse()
            .parse(&tokens)
            .into_result()
            .map_err(|errs| errs.into_iter().map(|e| e.into_owned()).collect())
    }

    fn _parse<'t>(&self) -> impl chumsky::Parser<'t, &'t [Token], ParseAst, Extra<'t>> {
        choice((
            env::env_block().map(Global::Env),
            model::model_block().map(Global::Model),
            api::api_block().map(Global::Api),
            service::service_block().map(Global::Service),
            poo_block().map(Global::Poo),
            data_source::data_source_block().map(Global::DataSource),
            inject_block().map(Global::Inject),
        ))
        .repeated()
        .collect::<Vec<_>>()
        .map(|items| {
            let mut ast = ParseAst::default();
            for item in items {
                match item {
                    Global::Env(env) => ast.wrangler_env.push(env),
                    Global::Model(model) => ast.models.push(model),
                    Global::Api(api) => ast.apis.push(api),
                    Global::Service(service) => ast.services.push(service),
                    Global::Poo(poo) => ast.poos.push(poo),
                    Global::DataSource(ds) => ast.sources.push(ds),
                    Global::Inject(symbols) => ast.injectables.extend(symbols),
                }
            }
            ast
        })
    }
}

enum Global {
    Env(WranglerEnvBlock),
    Model(ModelBlock),
    Api(ApiBlock),
    Service(ServiceBlock),
    Poo(PlainOldObjectBlock),
    DataSource(DataSourceBlock),
    Inject(Vec<UnresolvedName>),
}

/// Parses a block of the form:
///
/// ```cloesce
/// poo MyObject {
///     ident1: cidl_type
///     ident2: cidl_type
///     ...
/// }
/// ```
pub fn poo_block<'t>() -> impl Parser<'t, &'t [Token], PlainOldObjectBlock, Extra<'t>> {
    // ident: cidl_type
    let poo_field = select! { Token::Ident(name) => name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map(|((name, span), ty)| SpannedTypedName { span, name, ty });

    // poo MyObject { ... }
    just(Token::Poo)
        .ignore_then(
            select! { Token::Ident(name) => name }.map_with(|name, e| SpannedName {
                name,
                span: e.span(),
            }),
        )
        .then(
            poo_field
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|(span_name, fields)| PlainOldObjectBlock { span_name, fields })
}

/// Parses a block of the form:
///
/// ```cloesce
/// inject {
///     ident1
///     ident2
///     ...
/// }
/// ```
pub fn inject_block<'t>() -> impl Parser<'t, &'t [Token], Vec<UnresolvedName>, Extra<'t>> {
    // inject { ...}
    just(Token::Inject)
        .ignore_then(
            select! { Token::Ident(name) => name }
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|names| names.into_iter().map(UnresolvedName).collect())
}

/// Parses a CIDL type which can only be a SQLite column type
pub fn sqlite_column_types<'t>() -> impl Parser<'t, &'t [Token], CidlType, Extra<'t>> + Clone {
    choice((
        just(Token::String).to(CidlType::String),
        just(Token::Int).to(CidlType::Integer),
        just(Token::Double).to(CidlType::Double),
        just(Token::Date).to(CidlType::DateIso),
        just(Token::Bool).to(CidlType::Boolean),
    ))
}

pub fn cidl_type<'t>() -> impl Parser<'t, &'t [Token], CidlType, Extra<'t>> {
    recursive(|cidl_type| {
        let wrapper = select! { Token::Ident(name) => name }
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
            just(Token::Json).map(|_| CidlType::Json),
            just(Token::Void).map(|_| CidlType::Void),
            just(Token::Blob).map(|_| CidlType::Blob),
            just(Token::Stream).map(|_| CidlType::Stream),
            just(Token::R2Object).map(|_| CidlType::R2Object),
        ));

        let object_type = select! { Token::Ident(name) => CidlType::Object(name) };

        choice((wrapper, primitive_keyword, object_type)).boxed()
    })
}
