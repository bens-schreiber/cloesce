use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::atomic::AtomicU64;

use chumsky::extra::SimpleState;
use chumsky::prelude::*;

use ast::{Api, Binding, CidlType, CloesceAst, DataSource, Field, Model, PlainOldObject, Symbol, WranglerEnv};
use lexer::Token;

mod api;
mod model;
mod data_source;

const GLOBAL_SCOPE: &str = "global";
static GENSYM_SEED: AtomicU64 = AtomicU64::new(0);

pub struct SymbolTable {
    table: HashMap<String, Symbol>,
}

impl SymbolTable {
    pub fn new() -> Self {
        Self {
            table: HashMap::new(),
        }
    }

    fn gensym(prefix: &str) -> u32 {
        let id = GENSYM_SEED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let mut hasher = DefaultHasher::new();
        prefix.hash(&mut hasher);
        id.hash(&mut hasher);
        hasher.finish() as u32
    }

    pub fn intern_global(&mut self, name: &str) -> Symbol {
        self.intern_scoped(GLOBAL_SCOPE, name)
    }

    pub fn intern_scoped(&mut self, scope: &str, name: &str) -> Symbol {
        let key = format!("{}::{}", scope, name);
        if let Some(symbol) = self.table.get(&key) {
            symbol.clone()
        } else {
            let symbol = Symbol(Self::gensym(name));
            self.table.insert(key, symbol.clone());
            symbol
        }
    }
}

pub type Extra<'t> = extra::Full<Rich<'t, Token>, SimpleState<SymbolTable>, ()>;

#[derive(Default)]
pub struct CloesceParser;

impl CloesceParser {
    pub fn parse(
        &self,
        tokens: Vec<(Token, Range<usize>)>,
    ) -> Result<CloesceAst, Vec<Rich<'static, Token>>> {
        let tokens = tokens.into_iter().map(|(t, _)| t).collect::<Vec<_>>();
        let mut extra = SimpleState::from(SymbolTable::new());

        let parse_res = self
            ._parse(&tokens)
            .parse_with_state(&tokens, &mut extra)
            .into_result();

        match parse_res {
            Ok(ast) => Ok(ast),
            Err(errs) => Err(errs.into_iter().map(|e| e.into_owned()).collect()),
        }
    }

    fn _parse<'t>(
        &self,
        _tokens: &'t [Token],
    ) -> impl Parser<'t, &'t [Token], CloesceAst, Extra<'t>> {
        choice((
            Self::env_block().map(Global::Env),
            model::model_block().map(Global::Model),
            api::api_block().map(Global::Api),
            Self::poo_block().map(Global::Poo),
            data_source::data_source_block().map(Global::DataSource),
            Self::inject_block().map(Global::Inject),
        ))
        .repeated()
        .collect::<Vec<_>>()
        .map(|items| {
            let mut ast = CloesceAst::default();
            for item in items {
                match item {
                    Global::Env(env) => {
                        ast.wrangler_env.push(env);
                    }
                    Global::Model(model) => {
                        ast.models.insert(model.symbol.clone(), model);
                    }
                    Global::Api(api) => {
                        if let Some(existing) = ast.apis.get_mut(&api.symbol) {
                            for crud in api.cruds {
                                if !existing.cruds.contains(&crud) {
                                    existing.cruds.push(crud);
                                }
                            }

                            for method in api.methods {
                                if !existing
                                    .methods
                                    .iter()
                                    .any(|existing_method| existing_method.symbol == method.symbol)
                                {
                                    existing.methods.push(method);
                                }
                            }
                        } else {
                            ast.apis.insert(api.symbol.clone(), api);
                        }
                    }
                    Global::Poo(poo) => {
                        ast.poos.insert(poo.symbol.clone(), poo);
                    }
                    Global::DataSource(ds) => {
                        if let Some(existing) = ast.sources.get_mut(&ds.symbol) {
                            existing.push(ds);
                            continue;
                        }

                        ast.sources.insert(ds.symbol.clone(), vec![ds]);
                    }
                    Global::Inject(symbols) => {
                        ast.injectables.extend(symbols);
                    }
                }
            }
            ast
        })
    }

    /// Parses a POO (Plain Old Object) block of the form:
    /// ```cloesce
    /// poo MyObject {
    ///     field1: string
    ///     field2: MyOtherObject
    ///     anotherField: Option<User>
    /// }
    /// ```
    fn poo_block<'t>() -> impl Parser<'t, &'t [Token], PlainOldObject, Extra<'t>> {
        let poo_field = select! { Token::Ident(name) => name }
            .then_ignore(just(Token::Colon))
            .then(cidl_type())
            .map(|(name, cidl_type)| (name, cidl_type));

        just(Token::Poo)
            .ignore_then(select! { Token::Ident(name) => name })
            .then(
                poo_field
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|(poo_name, fields), e| {
                let symbol_table = e.state();
                let poo_symbol = symbol_table.intern_global(&poo_name);

                let attributes = fields
                    .into_iter()
                    .map(|(field_name, cidl_type)| {
                        let field_symbol = symbol_table.intern_scoped(&poo_name, &field_name);
                        Field {
                            symbol: field_symbol,
                            name: field_name,
                            cidl_type,
                        }
                    })
                    .collect();

                PlainOldObject {
                    symbol: poo_symbol,
                    name: poo_name,
                    attributes,
                    source_path: std::path::PathBuf::new(),
                }
            })
    }

    /// Parses an inject block of the form:
    /// ```cloesce
    /// inject {
    ///     OpenApiService
    ///     YouTubeApi
    /// }
    /// ```
    fn inject_block<'t>() -> impl Parser<'t, &'t [Token], Vec<Symbol>, Extra<'t>> {
        just(Token::Inject)
            .ignore_then(
                select! { Token::Ident(name) => name }
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|names, e| {
                let symbol_table: &mut SimpleState<SymbolTable> = e.state();
                let mut symbols = Vec::new();
                for name in names {
                    symbols.push(symbol_table.intern_global(&name));
                }
                symbols
            })
    }

    /// Parses an environment block of the form:
    fn env_block<'t>() -> impl Parser<'t, &'t [Token], WranglerEnv, Extra<'t>> {
        enum BindingKind {
            D1,
            R2,
            Kv,
        }

        enum EnvEntry {
            Binding(BindingKind),
            Var(CidlType),
        }

        const ENV_SCOPE: &str = "env";

        // Environment variables can be a sqlite column type or a JSON value
        let env_var = choice((
            sqlite_column_types(),
            just(Token::Json).map(|_| CidlType::Json),
        ));

        let env_entry = select! { Token::Ident(name) => name }
            .then_ignore(just(Token::Colon))
            .then(choice((
                just(Token::D1).map(|_| EnvEntry::Binding(BindingKind::D1)),
                just(Token::R2).map(|_| EnvEntry::Binding(BindingKind::R2)),
                just(Token::Kv).map(|_| EnvEntry::Binding(BindingKind::Kv)),
                env_var.map(EnvEntry::Var),
            )));

        just(Token::Env)
            .ignore_then(
                env_entry
                    .repeated()
                    .collect::<Vec<_>>()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map_with(|entries, e| {
                let symbol_table = e.state();
                let mut env = WranglerEnv {
                    symbol: symbol_table.intern_scoped(ENV_SCOPE, "default"),
                    d1_bindings: Vec::new(),
                    r2_bindings: Vec::new(),
                    kv_bindings: Vec::new(),
                    vars: HashMap::new(),
                };
                for (name, entry) in entries {
                    match entry {
                        EnvEntry::Binding(BindingKind::D1) => env.d1_bindings.push(Binding {
                            symbol: symbol_table.intern_scoped(ENV_SCOPE, &name),
                            name: name,
                        }),
                        EnvEntry::Binding(BindingKind::R2) => env.r2_bindings.push(Binding {
                            symbol: symbol_table.intern_scoped(ENV_SCOPE, &name),
                            name: name,
                        }),
                        EnvEntry::Binding(BindingKind::Kv) => env.kv_bindings.push(Binding {
                            symbol: symbol_table.intern_scoped(ENV_SCOPE, &name),
                            name: name,
                        }),
                        EnvEntry::Var(cidl_type) => {
                            let symbol = symbol_table.intern_scoped(ENV_SCOPE, &name);
                            env.vars.insert(
                                symbol.clone(),
                                Field {
                                    cidl_type,
                                    symbol: symbol,
                                    name: name,
                                },
                            );
                        }
                    }
                }
                env
            })
    }
}

fn sqlite_column_types<'t>() -> impl Parser<'t, &'t [Token], CidlType, Extra<'t>> + Clone {
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
            just(Token::Json).map(|_| CidlType::Json),
            just(Token::Void).map(|_| CidlType::Void),
            just(Token::Blob).map(|_| CidlType::Blob),
            just(Token::Stream).map(|_| CidlType::Stream),
            just(Token::R2Object).map(|_| CidlType::R2Object),
        ));

        let object_type = select! { Token::Ident(name) => CidlType::Object(name) };

        choice((generic_wrapper, primitive_keyword, object_type)).boxed()
    })
}

enum Global {
    Env(WranglerEnv),
    Model(Model),
    Api(Api),
    Poo(PlainOldObject),
    DataSource(DataSource),
    Inject(Vec<Symbol>),
}
