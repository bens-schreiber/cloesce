use frontend::{
    ApiBlock, ApiBlockMethodParamKind, ArgumentLiteral, Ast, AstBlockKind, DataSourceBlock,
    EnvBindingBlockKind, EnvBlock, InjectBlock, ModelBlock, PlainOldObjectBlock, Spd, SpdSlice,
    Symbol, Tag,
};
use idl::{
    CidlType, CloesceIdl, Field, Number, PlainOldObject, Service, ValidatedField, Validator,
    WranglerEnv,
};
use indexmap::IndexMap;

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::{
    api::ApiAnalysis,
    crud::CrudExpansion,
    data_source::{DataSourceAnalysis, DataSourceExpansion},
    err::{BatchResult, ErrorSink, SemanticError},
    model::ModelAnalysis,
};

mod api;
mod crud;
mod data_source;
pub mod err;
mod model;

pub struct SemanticAnalysis;
impl<'src, 'p> SemanticAnalysis {
    pub fn analyze(ast: &'p Ast<'src>) -> (CloesceIdl<'src>, Vec<SemanticError<'src, 'p>>) {
        let mut sink = ErrorSink::new();
        let table = SymbolTable::from_ast(ast, &mut sink);

        let wrangler_env = Self::wrangler(&table, &mut sink);

        let mut models = match ModelAnalysis::default().analyze(&table) {
            Ok(models) => models,
            Err(errs) => {
                sink.extend(errs);
                IndexMap::default()
            }
        };

        let data_source_map = DataSourceAnalysis::analyze(&models, &table, &mut sink);
        let poos = Self::poos(&table, &mut sink);

        let api_map = match ApiAnalysis::default().analyze(&table) {
            Ok(apis) => apis,
            Err(errs) => {
                sink.extend(errs);
                Vec::new()
            }
        };

        let mut services = Self::services(&table);

        // Merge API methods into their respective namespaces
        for (namespace, apis) in api_map {
            if let Some(model) = models.get_mut(&namespace) {
                model.apis.extend(apis);
            } else if let Some(service) = services.get_mut(&namespace) {
                service.apis.extend(apis);
            }
        }

        // Merge data sources into their respective models
        for (model_name, ds) in data_source_map {
            if let Some(model) = models.get_mut(&model_name) {
                model.data_sources.insert(ds.name, ds);
            }
        }

        let injects = table
            .injects
            .iter()
            .flat_map(|i| i.symbols.iter().map(|f| f.name))
            .collect();

        let mut idl = CloesceIdl {
            hash: 0,
            wrangler_env,
            models,
            services,
            poos,
            injects,
        };
        let errs = sink.drain();
        if !errs.is_empty() {
            return (idl, errs);
        }

        DataSourceExpansion::expand(&mut idl);
        CrudExpansion::expand(&mut idl);
        idl.set_merkle_hash();

        (idl, vec![])
    }

    fn wrangler(
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<WranglerEnv<'src>> {
        let mut d1_bindings = Vec::new();
        let mut r2_bindings = Vec::new();
        let mut kv_bindings = Vec::new();
        let mut vars = Vec::new();

        for block in table.envs.iter().flat_map(|e| e.blocks.inners()) {
            match block.kind {
                EnvBindingBlockKind::D1 => d1_bindings.extend(block.symbols.iter().map(|s| s.name)),
                EnvBindingBlockKind::R2 => r2_bindings.extend(block.symbols.iter().map(|s| s.name)),
                EnvBindingBlockKind::Kv => kv_bindings.extend(block.symbols.iter().map(|s| s.name)),
                EnvBindingBlockKind::Var => vars.extend(block.symbols.iter().map(|s| Field {
                    name: s.name.into(),
                    cidl_type: s.cidl_type.clone(),
                })),
            }
        }

        if d1_bindings.is_empty() && r2_bindings.is_empty() && kv_bindings.is_empty() {
            ensure!(
                table.models.is_empty(),
                sink,
                SemanticError::MissingWranglerEnvBlock
            );
            return None;
        };

        Some(WranglerEnv {
            d1_bindings,
            r2_bindings,
            kv_bindings,
            vars,
        })
    }

    fn poos(
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> BTreeMap<&'src str, PlainOldObject<'src>> {
        let mut res = BTreeMap::new();

        for poo in table.poos.values() {
            let poo_name = poo.symbol.name;
            let mut fields = Vec::new();

            for field in &poo.fields {
                let resolved_type = match resolve_cidl_type(field, &field.cidl_type, table) {
                    Ok(t) => t,
                    Err(err) => {
                        sink.push(err);
                        continue;
                    }
                };

                match resolved_type.root_type() {
                    CidlType::Stream => {
                        sink.push(SemanticError::PlainOldObjectInvalidFieldType { field });
                    }
                    _ => {
                        // All other types are valid
                    }
                }

                let validators = match resolve_validator_tags(field) {
                    Ok(v) => v,
                    Err(errs) => {
                        sink.extend(errs);
                        Vec::new()
                    }
                };

                fields.push(ValidatedField {
                    name: field.name.into(),
                    cidl_type: resolved_type,
                    validators,
                });
            }

            res.insert(
                poo_name,
                PlainOldObject {
                    name: poo_name,
                    fields,
                },
            );
        }

        res
    }

    fn services(table: &SymbolTable<'src, 'p>) -> IndexMap<&'src str, Service<'src>> {
        let mut res = IndexMap::new();
        for symbol in table.services.values() {
            res.insert(
                symbol.name,
                Service {
                    name: symbol.name,
                    apis: Vec::new(),
                },
            );
        }
        res
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Ord, PartialOrd)]
enum EnvBindingKind {
    D1,
    R2,
    Kv,
}

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
enum LocalSymbolKind<'src> {
    EnvVar(&'src str),
    EnvBinding {
        kind: EnvBindingKind,
        name: &'src str,
    },
    ModelField {
        model: &'src str,
        name: &'src str,
    },
    PlainOldObjectField {
        poo: &'src str,
        name: &'src str,
    },
    ApiMethodDecl {
        namespace: &'src str,
        name: &'src str,
    },
    ApiMethodParam {
        namespace: &'src str,
        method: &'src str,
        name: &'src str,
    },
    DataSourceDecl {
        model: &'src str,
        name: &'src str,
    },
    DataSourceMethodParam {
        data_source: &'src str,
        method: &'src str,
        name: &'src str,
    },
}

#[derive(Default)]
struct SymbolTable<'src, 'p> {
    // Globals
    models: BTreeMap<&'src str, &'p ModelBlock<'src>>,
    poos: BTreeMap<&'src str, &'p PlainOldObjectBlock<'src>>,
    services: BTreeMap<&'src str, &'p Symbol<'src>>,
    envs: Vec<&'p EnvBlock<'src>>,
    injects: Vec<&'p InjectBlock<'src>>,
    apis: Vec<&'p ApiBlock<'src>>,
    data_sources: Vec<&'p DataSourceBlock<'src>>,

    // Locals
    local: BTreeMap<LocalSymbolKind<'src>, &'p Symbol<'src>>,
}

impl<'src, 'p> SymbolTable<'src, 'p> {
    /// Creates a [SymbolTable] by walking the [Ast].
    ///
    /// Catches [SemanticError::DuplicateSymbol] errors.
    fn from_ast(ast: &'p Ast<'src>, sink: &mut ErrorSink<'src, 'p>) -> Self {
        let mut st = SymbolTable::default();
        let mut global_names = HashMap::new();

        let mut insert_global = |sink: &mut ErrorSink<'src, 'p>, symbol: &'p Symbol<'src>| {
            if let Some(first) = global_names.insert(symbol.name, symbol) {
                sink.push(SemanticError::DuplicateSymbol {
                    first,
                    second: symbol,
                });
                return false;
            }
            true
        };

        let mut insert_local = |sink: &mut ErrorSink<'src, 'p>,
                                symbol: &'p Symbol<'src>,
                                kind: LocalSymbolKind<'src>| {
            if let Some(first) = st.local.insert(kind, symbol) {
                sink.push(SemanticError::DuplicateSymbol {
                    first,
                    second: symbol,
                });
                return false;
            }
            true
        };

        for block in ast.blocks.inners() {
            match block {
                AstBlockKind::Model(model_block) => {
                    insert_global(sink, &model_block.symbol);
                    st.models.insert(model_block.symbol.name, model_block);

                    for symbol in model_block.blocks.iter().flat_map(|b| b.inner.symbols()) {
                        insert_local(
                            sink,
                            symbol,
                            LocalSymbolKind::ModelField {
                                model: model_block.symbol.name,
                                name: symbol.name,
                            },
                        );
                    }
                }
                AstBlockKind::PlainOldObject(plain_old_object_block) => {
                    insert_global(sink, &plain_old_object_block.symbol);
                    st.poos
                        .insert(plain_old_object_block.symbol.name, plain_old_object_block);

                    for field in &plain_old_object_block.fields {
                        insert_local(
                            sink,
                            field,
                            LocalSymbolKind::PlainOldObjectField {
                                poo: plain_old_object_block.symbol.name,
                                name: field.name,
                            },
                        );
                    }
                }
                AstBlockKind::Service(service_block) => {
                    for symbol in &service_block.symbols {
                        if insert_global(sink, symbol) {
                            st.services.insert(symbol.name, symbol);
                        }
                    }
                }
                AstBlockKind::Api(api_block) => {
                    st.apis.push(api_block);
                    for method in api_block.methods.inners() {
                        insert_local(
                            sink,
                            &method.symbol,
                            LocalSymbolKind::ApiMethodDecl {
                                namespace: api_block.symbol.name,
                                name: method.symbol.name,
                            },
                        );

                        for param in method.parameters.inners() {
                            let (symbol, name) = match param {
                                ApiBlockMethodParamKind::SelfParam(symbol) => (symbol, "self"),
                                ApiBlockMethodParamKind::Param(symbol) => (symbol, symbol.name),
                            };

                            insert_local(
                                sink,
                                symbol,
                                LocalSymbolKind::ApiMethodParam {
                                    namespace: api_block.symbol.name,
                                    method: method.symbol.name,
                                    name,
                                },
                            );
                        }
                    }
                }
                AstBlockKind::DataSource(data_source_block) => {
                    st.data_sources.push(data_source_block);
                    insert_local(
                        sink,
                        &data_source_block.symbol,
                        LocalSymbolKind::DataSourceDecl {
                            model: data_source_block.model.name,
                            name: data_source_block.symbol.name,
                        },
                    );

                    for (method_name, method) in [
                        ("list", &data_source_block.list),
                        ("get", &data_source_block.get),
                    ]
                    .into_iter()
                    .filter_map(|(n, m)| m.as_ref().map(|spd| (n, &spd.inner)))
                    {
                        for param in &method.parameters {
                            insert_local(
                                sink,
                                param,
                                LocalSymbolKind::DataSourceMethodParam {
                                    data_source: data_source_block.symbol.name,
                                    method: method_name,
                                    name: param.name,
                                },
                            );
                        }
                    }
                }
                AstBlockKind::Env(env_blocks) => {
                    st.envs.push(env_blocks);
                    for env_block in &env_blocks.blocks {
                        match &env_block.inner.kind {
                            EnvBindingBlockKind::D1 => {
                                for symbol in &env_block.inner.symbols {
                                    if insert_local(
                                        sink,
                                        symbol,
                                        LocalSymbolKind::EnvBinding {
                                            kind: EnvBindingKind::D1,
                                            name: symbol.name,
                                        },
                                    ) {
                                        insert_global(sink, symbol);
                                    }
                                }
                            }
                            EnvBindingBlockKind::R2 => {
                                for symbol in &env_block.inner.symbols {
                                    if insert_local(
                                        sink,
                                        symbol,
                                        LocalSymbolKind::EnvBinding {
                                            kind: EnvBindingKind::R2,
                                            name: symbol.name,
                                        },
                                    ) {
                                        insert_global(sink, symbol);
                                    }
                                }
                            }
                            EnvBindingBlockKind::Kv => {
                                for symbol in &env_block.inner.symbols {
                                    if insert_local(
                                        sink,
                                        symbol,
                                        LocalSymbolKind::EnvBinding {
                                            kind: EnvBindingKind::Kv,
                                            name: symbol.name,
                                        },
                                    ) {
                                        insert_global(sink, symbol);
                                    }
                                }
                            }
                            EnvBindingBlockKind::Var => {
                                for symbol in &env_block.inner.symbols {
                                    insert_local(
                                        sink,
                                        symbol,
                                        LocalSymbolKind::EnvVar(symbol.name),
                                    );
                                }
                            }
                        }
                    }
                }
                AstBlockKind::Inject(inject_block) => {
                    st.injects.push(inject_block);
                    for symbol in &inject_block.symbols {
                        insert_global(sink, symbol);
                    }
                }
            }
        }

        st
    }
}

/// Converts a [CidlType::UnresolvedReference] to a resolved type of [CidlType::Object] or [CidlType::Inject]
/// if possible, recursively. Also validates references inside of generic types.
///
/// Returns an error if the type cannot be resolved or is invalid.
fn resolve_cidl_type<'src, 'p>(
    symbol: &'p Symbol<'src>,
    cidl_type: &CidlType<'src>,
    table: &SymbolTable<'src, 'p>,
) -> Result<CidlType<'src>, SemanticError<'src, 'p>> {
    match cidl_type {
        CidlType::UnresolvedReference { name } => {
            if let Some(sym) = table.models.get(name) {
                return Ok(CidlType::Object {
                    name: sym.symbol.name,
                });
            }

            if let Some(sym) = table.poos.get(name) {
                return Ok(CidlType::Object {
                    name: sym.symbol.name,
                });
            }

            Err(SemanticError::UnresolvedSymbol { symbol })
        }
        CidlType::Partial { object_name } => {
            let valid =
                table.models.contains_key(object_name) || table.poos.contains_key(object_name);

            if !valid {
                return Err(SemanticError::UnresolvedSymbol { symbol });
            }
            Ok(cidl_type.clone())
        }
        CidlType::Nullable(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            Ok(CidlType::Nullable(Box::new(resolved_inner)))
        }
        CidlType::Array(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            Ok(CidlType::Array(Box::new(resolved_inner)))
        }
        CidlType::Paginated(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            Ok(CidlType::Paginated(Box::new(resolved_inner)))
        }
        CidlType::KvObject(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            Ok(CidlType::KvObject(Box::new(resolved_inner)))
        }
        _ => Ok(cidl_type.clone()),
    }
}

/// Resolves validators for a given symbol, returning an error if
/// any validator is invalid.
fn resolve_validator_tags<'src, 'p>(
    symbol: &'p Symbol<'src>,
) -> BatchResult<'src, 'p, Vec<Validator<'src>>> {
    use frontend::Keyword::*;

    let mut resolved = Vec::new();
    let mut sink = ErrorSink::new();

    for spd in &symbol.tags {
        let Tag::Validator {
            name: kw,
            argument: arg,
        } = &spd.inner
        else {
            continue;
        };

        let root = symbol.cidl_type.root_type();
        let type_ok = match kw {
            GreaterThan | GreaterThanOrEqual | LessThan | LessThanOrEqual | Step => {
                matches!(root, CidlType::Int | CidlType::Real)
            }
            Len | MinLen | MaxLen | Regex => matches!(root, CidlType::String),
            _ => unreachable!(
                "Tag::Validator only carries validator keywords as parsed in the frontend"
            ),
        };
        if !type_ok {
            sink.push(SemanticError::ValidatorInvalidForType {
                symbol,
                validator: spd,
            });
            continue;
        }

        let validator = match kw {
            GreaterThan => parse_number(arg, symbol, spd, &mut sink).map(Validator::GreaterThan),
            GreaterThanOrEqual => {
                parse_number(arg, symbol, spd, &mut sink).map(Validator::GreaterThanOrEqual)
            }
            LessThan => parse_number(arg, symbol, spd, &mut sink).map(Validator::LessThan),
            LessThanOrEqual => {
                parse_number(arg, symbol, spd, &mut sink).map(Validator::LessThanOrEqual)
            }
            Step => parse_i64(arg, symbol, spd, &mut sink).map(Validator::Step),
            Len => parse_usize(arg, symbol, spd, &mut sink).map(Validator::Length),
            MinLen => parse_usize(arg, symbol, spd, &mut sink).map(Validator::MinLength),
            MaxLen => parse_usize(arg, symbol, spd, &mut sink).map(Validator::MaxLength),
            Regex => match arg {
                ArgumentLiteral::Regex(s) => match regex::Regex::new(s) {
                    Ok(_) => Some(Validator::Regex(std::borrow::Cow::Borrowed(s))),
                    Err(e) => {
                        sink.push(SemanticError::ValidatorInvalidArgument {
                            symbol,
                            validator: spd,
                            reason: format!("invalid regex pattern: {e}"),
                        });
                        None
                    }
                },
                _ => {
                    sink.push(invalid_arg(
                        symbol,
                        spd,
                        "expected a regex argument (e.g. /pattern/)",
                    ));
                    None
                }
            },
            _ => unreachable!("non-validator keywords are filtered out above"),
        };

        if let Some(v) = validator {
            resolved.push(v);
        }
    }

    sink.finish()?;
    return Ok(resolved);

    fn invalid_arg<'src, 'p>(
        symbol: &'p Symbol<'src>,
        spd: &'p Spd<Tag<'src>>,
        reason: impl Into<std::string::String>,
    ) -> SemanticError<'src, 'p> {
        SemanticError::ValidatorInvalidArgument {
            symbol,
            validator: spd,
            reason: reason.into(),
        }
    }

    fn parse_number<'src, 'p>(
        lit: &ArgumentLiteral<'src>,
        symbol: &'p Symbol<'src>,
        spd: &'p Spd<Tag<'src>>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<Number> {
        match lit {
            ArgumentLiteral::Int(s) => match s.parse() {
                Ok(n) => return Some(Number::Int(n)),
                Err(_) => sink.push(invalid_arg(
                    symbol,
                    spd,
                    format!("'{s}' is not a valid integer"),
                )),
            },
            ArgumentLiteral::Real(s) => match s.parse() {
                Ok(n) => return Some(Number::Float(n)),
                Err(_) => sink.push(invalid_arg(
                    symbol,
                    spd,
                    format!("'{s}' is not a valid number"),
                )),
            },
            _ => sink.push(invalid_arg(symbol, spd, "expected a numeric argument")),
        }
        None
    }

    fn parse_usize<'src, 'p>(
        lit: &ArgumentLiteral<'src>,
        symbol: &'p Symbol<'src>,
        spd: &'p Spd<Tag<'src>>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<usize> {
        let ArgumentLiteral::Int(s) = lit else {
            sink.push(invalid_arg(symbol, spd, "expected an integer argument"));
            return None;
        };
        match s.parse() {
            Ok(n) => Some(n),
            Err(_) => {
                sink.push(invalid_arg(
                    symbol,
                    spd,
                    format!("'{s}' is not a valid non-negative integer"),
                ));
                None
            }
        }
    }

    fn parse_i64<'src, 'p>(
        lit: &ArgumentLiteral<'src>,
        symbol: &'p Symbol<'src>,
        spd: &'p Spd<Tag<'src>>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<i64> {
        let ArgumentLiteral::Int(s) = lit else {
            sink.push(invalid_arg(symbol, spd, "expected an integer argument"));
            return None;
        };
        match s.parse() {
            Ok(n) => Some(n),
            Err(_) => {
                sink.push(invalid_arg(
                    symbol,
                    spd,
                    format!("'{s}' is not a valid integer"),
                ));
                None
            }
        }
    }
}

/// Returns if a column in a D1 model is a valid SQLite type
fn is_valid_sql_type(cidl_type: &CidlType) -> bool {
    let inner = match cidl_type {
        CidlType::Nullable(inner) => inner.as_ref(),
        other => other,
    };

    matches!(
        inner,
        CidlType::Int
            | CidlType::Real
            | CidlType::String
            | CidlType::Blob
            | CidlType::Boolean
            | CidlType::DateIso
    )
}

// Kahns algorithm for topological sort + cycle detection.
// If no cycles, returns a map of name to position used for sorting the original collection.
fn kahns<'src, 'p>(
    graph: BTreeMap<&'src str, Vec<&'src str>>,
    mut in_degree: BTreeMap<&'src str, usize>,
    len: usize,
) -> Result<HashMap<&'src str, usize>, SemanticError<'src, 'p>> {
    let mut queue = in_degree
        .iter()
        .filter_map(|(&name, &deg)| (deg == 0).then_some(name))
        .collect::<VecDeque<_>>();

    let mut rank = HashMap::with_capacity(len);
    let mut counter = 0usize;

    while let Some(name) = queue.pop_front() {
        rank.insert(name, counter);
        counter += 1;

        if let Some(adjs) = graph.get(name) {
            for adj in adjs {
                let deg = in_degree.get_mut(adj).expect("names to be validated");
                *deg -= 1;

                if *deg == 0 {
                    queue.push_back(adj);
                }
            }
        }
    }

    if rank.len() != len {
        let cycle: Vec<&str> = in_degree
            .iter()
            .filter_map(|(&n, &d)| (d > 0).then_some(n))
            .collect();

        if !cycle.is_empty() {
            return Err(SemanticError::CyclicalRelationship { cycle });
        }
    }

    Ok(rank)
}
