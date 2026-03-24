use std::{cell::RefCell, collections::HashMap, ops::Range, path::PathBuf, rc::Rc};

use ast::CidlType;
use chumsky::extra;
use chumsky::prelude::*;

use crate::SpannedTypedName;
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
mod service;

pub type ParseId = usize;

#[derive(Clone)]
pub enum IdScope {
    Global,
    Env,
    Inject,
    Model(String),
    Api(String),
    DataSource(String),
    Service(String),
    PlainOldObject(String),
}

#[derive(Clone)]
pub struct IdTable {
    counter: usize,
    table: HashMap<String, ParseId>,
    names: HashMap<ParseId, String>,
}

impl Default for IdTable {
    fn default() -> Self {
        Self {
            counter: 0,
            table: HashMap::new(),
            names: HashMap::new(),
        }
    }
}

impl IdTable {
    fn _key(name: &str, scope: &IdScope) -> String {
        match scope {
            IdScope::Global => format!("global::{}", name),
            IdScope::Env => format!("env::{}", name),
            IdScope::Inject => format!("inject::{}", name),
            IdScope::Model(model_name) => format!("model::{}::{}", model_name, name),
            IdScope::Api(api_name) => format!("api::{}::{}", api_name, name),
            IdScope::Service(service_name) => format!("service::{}::{}", service_name, name),
            IdScope::PlainOldObject(poo_name) => format!("poo::{}::{}", poo_name, name),
            IdScope::DataSource(ds_name) => format!("source::{}::{}", ds_name, name),
        }
    }

    pub fn intern(&mut self, name: String, scope: IdScope) -> ParseId {
        let key = Self::_key(&name, &scope);
        if let Some(&existing) = self.table.get(&key) {
            return existing;
        }
        let symbol_ref = self.counter;
        self.counter += 1;
        self.names.entry(symbol_ref).or_insert_with(|| name.clone());
        self.table.insert(key, symbol_ref);
        symbol_ref
    }

    pub fn new_id(&mut self) -> ParseId {
        let id = self.counter;
        self.counter += 1;
        id
    }

    pub fn name_of(&self, id: ParseId) -> Option<&str> {
        self.names.get(&id).map(|s| s.as_str())
    }

    pub fn id(&self, name: &str) -> ParseId {
        let key = format!("global::{}", name);
        *self
            .table
            .get(&key)
            .unwrap_or_else(|| panic!("global symbol '{}' not found in symbol table", name))
    }
}

pub type It = Rc<RefCell<IdTable>>;

#[derive(Default)]
pub struct CloesceParser;

impl CloesceParser {
    pub fn parse(
        &self,
        tokens: Vec<(Token, Range<usize>)>,
    ) -> Result<(ParseAst, IdTable), Vec<Rich<'static, Token>>> {
        let tokens = tokens.into_iter().map(|(t, _)| t).collect::<Vec<_>>();
        let it: It = Rc::new(RefCell::new(IdTable::default()));
        let result = self
            ._parse(it.clone())
            .parse(&tokens)
            .into_result()
            .map_err(|errs| errs.into_iter().map(|e| e.into_owned()).collect::<Vec<_>>())?;
        let sym_table = it.borrow().clone();
        Ok((result, sym_table))
    }

    fn _parse<'t>(&self, it: It) -> impl chumsky::Parser<'t, &'t [Token], ParseAst, Extra<'t>> {
        choice((
            env::env_block(it.clone()).map(Global::Env),
            model::model_block(it.clone()).map(Global::Model),
            api::api_block(it.clone()).map(Global::Api),
            service::service_block(it.clone()).map(Global::Service),
            poo_block(it.clone()).map(Global::Poo),
            data_source::data_source_block(it.clone()).map(Global::DataSource),
            inject_block(it).map(Global::Inject),
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
pub(crate) fn poo_block<'t>(
    st: It,
) -> impl Parser<'t, &'t [Token], PlainOldObjectBlock, Extra<'t>> {
    let st_fields = st.clone();
    let st_id = st.clone();

    // ident: cidl_type
    let poo_field = select! { Token::Ident(name) => name }
        .map_with(|name, e| (name, e.span()))
        .then_ignore(just(Token::Colon))
        .then(cidl_type(st_fields))
        .map(|((name, span), ty)| SpannedTypedName {
            id: 0, // resolved in outer map
            span,
            name,
            cidl_type: ty,
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
        .map(move |((name, span), mut fields)| {
            let id = st_id.borrow_mut().intern(name.clone(), IdScope::Global);
            let poo_scope = IdScope::PlainOldObject(name.clone());
            for f in &mut fields {
                f.id = st_id.borrow_mut().intern(f.name.clone(), poo_scope.clone());
            }
            PlainOldObjectBlock {
                id,
                span,
                name,
                file: PathBuf::new(),
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
pub(crate) fn inject_block<'t>(st: It) -> impl Parser<'t, &'t [Token], InjectBlock, Extra<'t>> {
    // inject { ...}
    just(Token::Inject)
        .ignore_then(
            select! { Token::Ident(name) => name }
                .repeated()
                .collect::<Vec<_>>()
                .delimited_by(just(Token::LBrace), just(Token::RBrace)),
        )
        .map_with(move |injectables, e| {
            let mut table = st.borrow_mut();
            let id = table.new_id();
            let names = injectables
                .into_iter()
                .map(|name| table.intern(name, IdScope::Inject))
                .collect();
            InjectBlock {
                id,
                span: e.span(),
                file: PathBuf::new(),
                refs: names,
            }
        })
}

pub(crate) fn cidl_type<'t>(st: It) -> impl Parser<'t, &'t [Token], CidlType, Extra<'t>> {
    recursive(move |cidl_type| {
        let st_obj = st.clone();

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
                    CidlType::Object(sym) => Ok(CidlType::Partial(sym)),
                    _ => Err(Rich::custom(span, "Partial<T> expects an object type")),
                },
                "DataSource" => match inner {
                    CidlType::Object(sym) => Ok(CidlType::DataSource(sym)),
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
        ));

        let object_type = select! { Token::Ident(name) => name }
            .map(move |name| CidlType::Object(st_obj.borrow_mut().intern(name, IdScope::Global)));

        choice((wrapper, primitive_keyword, object_type)).boxed()
    })
}
