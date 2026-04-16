use ast::{CidlType, CloesceAst, Field, PlainOldObject, Service, WranglerEnv};
use frontend::{
    ApiBlock, ApiBlockMethodParamKind, AstBlockKind, DataSourceBlock, EnvBindingKind, EnvBlock,
    EnvBlockKind, InjectBlock, ModelBlock, ParseAst, PlainOldObjectBlock, ServiceBlock, Spd,
    SpdSlice, Symbol,
};
use indexmap::IndexMap;

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::{
    api::ApiAnalysis,
    crud::CrudExpansion,
    data_source::{DataSourceAnalysis, DataSourceExpansion},
    err::{ErrorSink, SemanticError},
    model::ModelAnalysis,
};

mod api;
mod crud;
mod data_source;
pub mod err;
mod model;

#[derive(Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum SymbolKind<'src> {
    // Scoped
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
    ServiceField {
        service: &'src str,
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

type SymbolLookup<'src, 'p> = BTreeMap<SymbolKind<'src>, &'p Symbol<'src>>;

#[derive(Default)]
pub struct SymbolTable<'src, 'p> {
    models: BTreeMap<&'src str, &'p ModelBlock<'src>>,
    poos: BTreeMap<&'src str, &'p PlainOldObjectBlock<'src>>,
    services: BTreeMap<&'src str, &'p ServiceBlock<'src>>,
    envs: Vec<&'p Vec<Spd<EnvBlock<'src>>>>,
    injects: Vec<&'p InjectBlock<'src>>,
    data_sources: BTreeMap<SymbolKind<'src>, &'p DataSourceBlock<'src>>,
    apis: Vec<&'p ApiBlock<'src>>,

    env_vars: SymbolLookup<'src, 'p>,
    env_bindings: SymbolLookup<'src, 'p>,
    model_fields: SymbolLookup<'src, 'p>,
    poo_fields: SymbolLookup<'src, 'p>,
    service_fields: SymbolLookup<'src, 'p>,
    api_method_decls: SymbolLookup<'src, 'p>,
    api_method_params: SymbolLookup<'src, 'p>,
    data_source_method_params: SymbolLookup<'src, 'p>,
}

impl<'src, 'p> SymbolTable<'src, 'p> {
    /// Creates a [SymbolTable] from a [ParseAst], catching [SemanticError::DuplicateSymbol]'s
    fn from_parse(parse: &'p ParseAst<'src>, sink: &mut ErrorSink<'src, 'p>) -> Self {
        let mut st = SymbolTable::default();
        let mut global_names = HashMap::new();

        let mut insert_global = |sink: &mut ErrorSink<'src, 'p>, symbol: &'p Symbol<'src>| {
            if let Some(first) = global_names.insert(symbol.name, symbol) {
                sink.push(SemanticError::DuplicateSymbol {
                    first,
                    second: symbol,
                });
            }
        };

        for block in parse.blocks.blocks() {
            match block {
                AstBlockKind::Model(model_block) => {
                    insert_global(sink, &model_block.symbol);
                    st.models.insert(model_block.symbol.name, model_block);

                    for sub_block in model_block.blocks.blocks() {
                        let symbols = sub_block.symbols();
                        for symbol in symbols {
                            if let Some(first) = st.model_fields.insert(
                                SymbolKind::ModelField {
                                    model: model_block.symbol.name,
                                    name: symbol.name,
                                },
                                symbol,
                            ) {
                                sink.push(SemanticError::DuplicateSymbol {
                                    first,
                                    second: symbol,
                                });
                            }
                        }
                    }
                }
                AstBlockKind::PlainOldObject(plain_old_object_block) => {
                    insert_global(sink, &plain_old_object_block.symbol);
                    st.poos
                        .insert(plain_old_object_block.symbol.name, plain_old_object_block);

                    for field in &plain_old_object_block.fields {
                        if let Some(first) = st.poo_fields.insert(
                            SymbolKind::PlainOldObjectField {
                                poo: plain_old_object_block.symbol.name,
                                name: field.name,
                            },
                            field,
                        ) {
                            sink.push(SemanticError::DuplicateSymbol {
                                first,
                                second: field,
                            });
                        }
                    }
                }
                AstBlockKind::Service(service_block) => {
                    insert_global(sink, &service_block.symbol);
                    st.services.insert(service_block.symbol.name, service_block);

                    for field in &service_block.fields {
                        if let Some(first) = st.service_fields.insert(
                            SymbolKind::ServiceField {
                                service: service_block.symbol.name,
                                name: field.name,
                            },
                            field,
                        ) {
                            sink.push(SemanticError::DuplicateSymbol {
                                first,
                                second: field,
                            });
                        }
                    }
                }
                AstBlockKind::Api(api_block) => {
                    st.apis.push(api_block);
                    for method in api_block.methods.blocks() {
                        if let Some(first) = st.api_method_decls.insert(
                            SymbolKind::ApiMethodDecl {
                                namespace: api_block.symbol.name,
                                name: method.symbol.name,
                            },
                            &method.symbol,
                        ) {
                            sink.push(SemanticError::DuplicateSymbol {
                                first,
                                second: &method.symbol,
                            });
                        }

                        for param in method.parameters.blocks() {
                            let (symbol, name) = match param {
                                ApiBlockMethodParamKind::SelfParam { symbol, .. } => {
                                    (symbol, "self")
                                }
                                ApiBlockMethodParamKind::Field(symbol) => (symbol, symbol.name),
                            };

                            if let Some(first) = st.api_method_params.insert(
                                SymbolKind::ApiMethodParam {
                                    namespace: api_block.symbol.name,
                                    method: method.symbol.name,
                                    name,
                                },
                                symbol,
                            ) {
                                sink.push(SemanticError::DuplicateSymbol {
                                    first,
                                    second: symbol,
                                });
                            }
                        }
                    }
                }
                AstBlockKind::DataSource(data_source_block) => {
                    if let Some(first) = st.data_sources.insert(
                        SymbolKind::DataSourceDecl {
                            model: data_source_block.model.name,
                            name: data_source_block.symbol.name,
                        },
                        data_source_block,
                    ) {
                        sink.push(SemanticError::DuplicateSymbol {
                            first: &first.symbol,
                            second: &data_source_block.symbol,
                        });
                    }

                    for (method_name, method) in [
                        ("list", &data_source_block.list),
                        ("get", &data_source_block.get),
                    ]
                    .into_iter()
                    .filter_map(|(n, m)| m.as_ref().map(|spd| (n, &spd.block)))
                    {
                        for param in &method.parameters {
                            if let Some(first) = st.data_source_method_params.insert(
                                SymbolKind::DataSourceMethodParam {
                                    data_source: data_source_block.symbol.name,
                                    method: method_name,
                                    name: param.name,
                                },
                                param,
                            ) {
                                sink.push(SemanticError::DuplicateSymbol {
                                    first,
                                    second: param,
                                });
                            }
                        }
                    }
                }
                AstBlockKind::Env(env_blocks) => {
                    st.envs.push(env_blocks);
                    for env_block in env_blocks.blocks() {
                        match &env_block.kind {
                            EnvBlockKind::D1 => {
                                for symbol in &env_block.symbols {
                                    insert_global(sink, symbol);
                                    st.env_bindings.insert(
                                        SymbolKind::EnvBinding {
                                            kind: EnvBindingKind::D1,
                                            name: symbol.name,
                                        },
                                        symbol,
                                    );
                                }
                            }
                            EnvBlockKind::R2 => {
                                for symbol in &env_block.symbols {
                                    insert_global(sink, symbol);
                                    st.env_bindings.insert(
                                        SymbolKind::EnvBinding {
                                            kind: EnvBindingKind::R2,
                                            name: symbol.name,
                                        },
                                        symbol,
                                    );
                                }
                            }
                            EnvBlockKind::Kv => {
                                for symbol in &env_block.symbols {
                                    insert_global(sink, symbol);
                                    st.env_bindings.insert(
                                        SymbolKind::EnvBinding {
                                            kind: EnvBindingKind::Kv,
                                            name: symbol.name,
                                        },
                                        symbol,
                                    );
                                }
                            }
                            EnvBlockKind::Var => {
                                for symbol in &env_block.symbols {
                                    if let Some(first) =
                                        st.env_vars.insert(SymbolKind::EnvVar(symbol.name), symbol)
                                    {
                                        sink.push(SemanticError::DuplicateSymbol {
                                            first,
                                            second: symbol,
                                        });
                                    }
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

pub struct SemanticAnalysis;
impl<'src, 'p> SemanticAnalysis {
    pub fn analyze(parse: &'p ParseAst<'src>) -> (CloesceAst<'src>, Vec<SemanticError<'src, 'p>>) {
        let mut sink = ErrorSink::new();
        let table = SymbolTable::from_parse(parse, &mut sink);

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

        let mut services = Self::services(&table, &mut sink);

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

        let mut ast = CloesceAst {
            hash: 0,
            wrangler_env,
            models,
            services,
            poos,
            injects,
        };
        let errs = sink.drain();
        if !errs.is_empty() {
            return (ast, errs);
        }

        DataSourceExpansion::expand(&mut ast);
        CrudExpansion::expand(&mut ast);
        ast.set_merkle_hash();

        (ast, vec![])
    }

    fn wrangler(
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<WranglerEnv<'src>> {
        if table.env_bindings.is_empty() {
            ensure!(
                table.models.is_empty(),
                sink,
                SemanticError::MissingWranglerEnvBlock
            );
            return None;
        };

        let mut d1_bindings = Vec::new();
        let mut r2_bindings = Vec::new();
        let mut kv_bindings = Vec::new();
        for symbol_kind in table.env_bindings.keys() {
            if let SymbolKind::EnvBinding { kind, name } = symbol_kind {
                match kind {
                    EnvBindingKind::D1 => d1_bindings.push(*name),
                    EnvBindingKind::R2 => r2_bindings.push(*name),
                    EnvBindingKind::Kv => kv_bindings.push(*name),
                }
            } else {
                unreachable!("only EnvBinding kinds should be in env_bindings")
            }
        }

        let vars = table
            .env_vars
            .values()
            .map(|symbol| Field {
                name: symbol.name.into(),
                cidl_type: symbol.cidl_type.clone(),
            })
            .collect();

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

        // Cycle detection
        let mut in_degree = BTreeMap::<&str, usize>::new();
        let mut graph = BTreeMap::<&str, Vec<&str>>::new();

        for poo in table.poos.values() {
            let poo_name = poo.symbol.name;
            let mut fields = Vec::new();
            graph.entry(poo_name).or_default();
            in_degree.entry(poo_name).or_insert(0);

            for field in &poo.fields {
                let resolved_type = match resolve_cidl_type(field, &field.cidl_type, table) {
                    Ok(t) => t,
                    Err(err) => {
                        sink.push(err);
                        continue;
                    }
                };

                match resolved_type.root_type() {
                    CidlType::Object { name, .. } if table.poos.contains_key(name) => {
                        graph.entry(name).or_default().push(poo_name);
                        in_degree.entry(poo_name).and_modify(|d| *d += 1);
                    }
                    CidlType::Stream | CidlType::Void => {
                        sink.push(SemanticError::PlainOldObjectInvalidFieldType { field });
                    }
                    _ => {
                        // All other types are valid
                    }
                }

                fields.push(Field {
                    name: field.name.into(),
                    cidl_type: resolved_type,
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

        match kahns(graph, in_degree, table.poos.len()) {
            Ok(_) => res,
            Err(err) => {
                sink.push(err);
                BTreeMap::new()
            }
        }
    }

    fn services(
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> IndexMap<&'src str, Service<'src>> {
        let mut res = IndexMap::new();

        // Cycle detection via Kahn's
        let mut in_degree = BTreeMap::<&str, usize>::new();
        let mut graph = BTreeMap::<&str, Vec<&str>>::new();

        for service in table.services.values() {
            let service_name = service.symbol.name;
            let mut fields = Vec::new();
            graph.entry(service_name).or_default();
            in_degree.entry(service_name).or_insert(0);

            for field in &service.fields {
                let resolved_type = match resolve_cidl_type(field, &field.cidl_type, table) {
                    Ok(t) => t,
                    Err(err) => {
                        sink.push(err);
                        continue;
                    }
                };

                if let CidlType::Inject { name } = resolved_type
                    && table.services.contains_key(name)
                {
                    graph.entry(name).or_default().push(service_name);
                    in_degree.entry(service_name).and_modify(|d| *d += 1);
                }

                fields.push(Field {
                    name: field.name.into(),
                    cidl_type: resolved_type,
                });
            }

            res.insert(
                service_name,
                Service {
                    name: service_name,
                    fields,
                    apis: Vec::new(),
                },
            );
        }

        match kahns(graph, in_degree, table.services.len()) {
            Ok(rank) => {
                res.sort_by(|a, _, b, _| rank[a].cmp(&rank[b]));
                res
            }
            Err(err) => {
                sink.push(err);
                IndexMap::new()
            }
        }
    }
}

/// Converts a [CidlType::UnresolvedReference] to a resolved type of [CidlType::Object] or [CidlType::Inject]
/// if possible, recursively. Also validates [CidlType::DataSource] and [CidlType::Partial] references.
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

            if let Some(sym) = table.services.get(name) {
                return Ok(CidlType::Inject {
                    name: sym.symbol.name,
                });
            }

            if let Some(sym) = table
                .injects
                .iter()
                .flat_map(|i| i.symbols.iter())
                .find(|s| s.name == *name)
            {
                return Ok(CidlType::Inject { name: sym.name });
            }

            Err(SemanticError::UnresolvedSymbol { symbol })
        }
        CidlType::DataSource { model_name } => {
            let valid = table.models.contains_key(model_name);

            if !valid {
                return Err(SemanticError::UnresolvedSymbol { symbol });
            }
            Ok(cidl_type.clone())
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

/// Returns if a column in a D1 model is a valid SQLite type
fn is_valid_sql_type(cidl_type: &CidlType) -> bool {
    let inner = match cidl_type {
        CidlType::Nullable(inner) => inner.as_ref(),
        other => other,
    };

    matches!(
        inner,
        CidlType::Integer
            | CidlType::Double
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
