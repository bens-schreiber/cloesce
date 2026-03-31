use std::ops::Range;

use ast::CidlType;
use chumsky::extra;
use chumsky::prelude::*;

use crate::FileSpan;
use crate::Symbol;
use crate::SymbolKind;
use crate::lexer::Token;
use crate::{
    ApiBlock, DataSourceBlock, InjectBlock, ModelBlock, ParseAst, PlainOldObjectBlock,
    ServiceBlock, WranglerEnvBlock,
};

pub(crate) type Extra<'t> = extra::Err<Rich<'t, Token>>;

mod api;
mod data_source;
mod env;
mod model;

pub struct CloesceParser;

impl CloesceParser {
    pub fn parse(
        &self,
        tokens: Vec<(Token, Range<usize>)>,
    ) -> Result<ParseAst, Vec<Rich<'static, Token>>> {
        let tokens = tokens.into_iter().map(|(t, _)| t).collect::<Vec<_>>();
        let result = self
            ._parse()
            .parse(&tokens)
            .into_result()
            .map_err(|errs| errs.into_iter().map(|e| e.into_owned()).collect::<Vec<_>>())?;
        Ok(result)
    }

    fn _parse<'t>(&self) -> impl chumsky::Parser<'t, &'t [Token], ParseAst, Extra<'t>> {
        choice((
            env::env_block().map(Global::Env),
            model::model_block().map(Global::Model),
            api::api_block().map(Global::Api),
            data_source::data_source_block().map(Global::DataSource),
            service_block().map(Global::Service),
            poo_block().map(Global::Poo),
            inject_block().map(Global::Inject),
        ))
        .repeated()
        .collect::<Vec<_>>()
        .map(|items| {
            let mut ast = ParseAst::default();
            for item in items {
                match item {
                    Global::Env(env) => ast.wrangler_envs.push(env),
                    Global::Model(model) => ast.models.push(model),
                    Global::Api(api) => ast.apis.push(api),
                    Global::Service(service) => ast.services.push(service),
                    Global::Poo(poo) => ast.poos.push(poo),
                    Global::DataSource(ds) => ast.sources.push(ds),
                    Global::Inject(block) => ast.injects.push(block),
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
    Inject(InjectBlock),
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
fn poo_block<'t>() -> impl Parser<'t, &'t [Token], PlainOldObjectBlock, Extra<'t>> {
    // ident: cidl_type
    let poo_field = select! { Token::Ident(name) => name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(cidl_type())
        .map(|((name, span), ty)| Symbol {
            span: FileSpan::from_simple_span(span),
            name,
            cidl_type: ty,
            kind: SymbolKind::PlainOldObjectField,
            ..Default::default()
        });

    // poo MyObject { ... }
    just(Token::Poo)
        .ignore_then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then(
            poo_field
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(|((name, span), mut fields)| {
            for field in &mut fields {
                field.parent_name = name.clone();
            }

            PlainOldObjectBlock {
                symbol: Symbol {
                    span: FileSpan::from_simple_span(span),
                    name,
                    kind: SymbolKind::PlainOldObjectDecl,
                    ..Default::default()
                },
                fields,
            }
        })
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
fn inject_block<'t>() -> impl Parser<'t, &'t [Token], InjectBlock, Extra<'t>> {
    // inject { ...}
    just(Token::Inject)
        .ignore_then(
            select! { Token::Ident(name) => name }
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(move |fields, e| InjectBlock {
            symbol: Symbol {
                span: FileSpan::from_simple_span(e.span()),
                kind: SymbolKind::InjectDecl,
                ..Default::default()
            },
            fields: fields
                .into_iter()
                .map(|field_name| Symbol {
                    span: FileSpan::from_simple_span(e.span()),
                    name: field_name,
                    kind: SymbolKind::InjectDecl,
                    ..Default::default()
                })
                .collect(),
        })
}

/// Parses a block of the form:
///
/// ```cloesce
/// service MyAppService {
///     ident1: InjectedService1
///     ident2: InjectedService2
/// }
/// ```
pub fn service_block<'t>() -> impl Parser<'t, &'t [Token], ServiceBlock, Extra<'t>> {
    // ident: InjectedService
    let attribute = select! { Token::Ident(var_name) => var_name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(cidl_type());

    // service ServiceName { ... }
    just(Token::Service)
        .ignore_then(select! { Token::Ident(name) => name }.map_with(|name, e| (name, e.span())))
        .then(
            attribute
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map(move |((name, span), fields)| {
            let fields = fields
                .into_iter()
                .map(|((name, span), cidl_type)| Symbol {
                    span: FileSpan::from_simple_span(span),
                    name,
                    cidl_type,
                    kind: SymbolKind::ServiceField,
                    ..Default::default()
                })
                .collect();

            ServiceBlock {
                symbol: Symbol {
                    span: FileSpan::from_simple_span(span),
                    name,
                    kind: SymbolKind::ServiceDecl,
                    ..Default::default()
                },
                fields,
            }
        })
}

fn cidl_type<'t>() -> impl Parser<'t, &'t [Token], CidlType, Extra<'t>> {
    recursive(move |cidl_type| {
        let wrapper = select! { Token::Ident(name) => name }
            .then_ignore(just(Token::LAngle))
            .then(cidl_type.clone())
            .then_ignore(just(Token::RAngle))
            .try_map(|(wrapper, inner), span| match wrapper.as_str() {
                "Option" => Ok(CidlType::nullable(inner)),
                "Array" => Ok(CidlType::array(inner)),
                "Paginated" => Ok(CidlType::paginated(inner)),
                "KvObject" => Ok(CidlType::KvObject(Box::new(inner))),
                "Partial" => match inner {
                    CidlType::UnresolvedReference { name } => {
                        Ok(CidlType::Partial { object_name: name })
                    }
                    _ => Err(Rich::custom(span, "Partial<T> expects an object type")),
                },
                "DataSource" => match inner {
                    CidlType::UnresolvedReference { name: model_name } => {
                        Ok(CidlType::DataSource { model_name })
                    }
                    _ => Err(Rich::custom(span, "DataSource<T> expects an object type")),
                },
                _ => Err(Rich::custom(span, "Unknown generic type wrapper")),
            });

        let primitive_keyword = choice((
            just(Token::String).to(CidlType::String),
            just(Token::Int).to(CidlType::Integer),
            just(Token::Double).to(CidlType::Double),
            just(Token::Date).to(CidlType::DateIso),
            just(Token::Bool).to(CidlType::Boolean),
            just(Token::Json).to(CidlType::Json),
            just(Token::Void).to(CidlType::Void),
            just(Token::Blob).to(CidlType::Blob),
            just(Token::Stream).to(CidlType::Stream),
            just(Token::R2Object).to(CidlType::R2Object),
            just(Token::Env).to(CidlType::Env),
        ));

        let unresolved_type = select! { Token::Ident(name) => name }
            .map(move |name| CidlType::UnresolvedReference { name: name.clone() });

        choice((wrapper, primitive_keyword, unresolved_type)).boxed()
    })
}
