use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::sync::atomic::AtomicU64;

use chumsky::extra::SimpleState;
use chumsky::prelude::*;

use ast::{Binding, CidlType, CloesceAst, Field, Model, Symbol, WranglerEnv};
use lexer::Token;

mod model;

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
                }
            }
            ast
        })
    }

    /// Parses an environment block of the form:
    /// ```cloesce
    /// env {
    ///     // Bindings (d1, r2, kv)
    ///     my_d1_binding: d1,
    ///     my_r2_binding: r2,
    ///     my_kv_binding: kv,
    ///
    ///     // Variables (any non nested CIDL type)
    ///     my_var: string,
    ///     my_other_var: int,
    ///     ...
    /// }
    /// ```
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
            just(Token::Json).map(|_| CidlType::JsonValue),
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

fn sqlite_column_types<'t>() -> impl Parser<'t, &'t [Token], CidlType, Extra<'t>> {
    choice((
        just(Token::String).to(CidlType::String),
        just(Token::Int).to(CidlType::Integer),
        just(Token::Double).to(CidlType::Double),
        just(Token::Date).to(CidlType::DateIso),
        just(Token::Bool).to(CidlType::Boolean),
    ))
}

enum Global {
    Env(WranglerEnv),
    Model(Model),
}
