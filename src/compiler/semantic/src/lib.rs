use ast::{
    Api, CidlType, CloesceAst, DataSource, DataSourceMethod, Field, IncludeTree, Model,
    PlainOldObject, Service, ServiceField, WranglerEnv,
};
use frontend::{ModelBlock, ParseAst};
use indexmap::IndexMap;

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::{
    api::ApiAnalysis,
    err::{CompilerErrorKind, ErrorSink},
    model::ModelAnalysis,
};

mod api;
pub mod err;
mod model;

pub use frontend::{FileSpan, Symbol, SymbolKind, WranglerEnvBindingKind};

#[derive(Default)]
pub struct SymbolTable {
    table: HashMap<String, Symbol>,
}

impl SymbolTable {
    fn key(&self, symbol: &Symbol) -> String {
        match symbol.kind {
            // Global symbols
            SymbolKind::ModelDecl
            | SymbolKind::PlainOldObjectDecl
            | SymbolKind::ServiceDecl
            | SymbolKind::ApiDecl
            | SymbolKind::DataSourceDecl
            | SymbolKind::WranglerEnvBinding { .. }
            | SymbolKind::WranglerEnvDecl
            | SymbolKind::InjectDecl => symbol.name.clone(),

            // Scoped symbols
            SymbolKind::ModelField
            | SymbolKind::ServiceField
            | SymbolKind::ApiMethodDecl
            | SymbolKind::ApiMethodParam
            | SymbolKind::PlainOldObjectField
            | SymbolKind::DataSourceMethodParam
            | SymbolKind::DataSourceMethodDecl
            | SymbolKind::WranglerEnvVar => {
                format!("{}::{}", symbol.parent_name, symbol.name)
            }

            _ => panic!("cannot generate key for Null symbol"),
        }
    }

    /// Resolves a name to a symbol
    fn resolve(&self, name: &str, kind: SymbolKind, parent_name: Option<&str>) -> Option<&Symbol> {
        let key = self.key(&Symbol {
            name: name.to_string(),
            kind: kind.clone(),
            parent_name: parent_name.unwrap_or("").to_string(),
            ..Default::default()
        });

        let symbol = self.table.get(&key);
        if let Some(symbol) = symbol {
            match (&kind, &symbol.kind) {
                (
                    SymbolKind::WranglerEnvBinding { kind: got },
                    SymbolKind::WranglerEnvBinding { kind: found },
                ) if got != found => {
                    // Wrangler env bindings are all in the same scope (so names can collide), but
                    // bindings of different kinds cannot resolve to each other
                    return None;
                }
                _ => {}
            };
        }

        symbol
    }

    /// Creates a [SymbolTable] from a [ParseAst], catching [CompilerErrorKind::DuplicateSymbol]'s
    fn from_parse(parse: &ParseAst, sink: &mut ErrorSink) -> SymbolTable {
        let mut st = SymbolTable::default();

        let insert_unique = |st: &mut SymbolTable, sink: &mut ErrorSink, symbol: &Symbol| {
            if let Some(existing) = st.table.insert(st.key(&symbol), symbol.clone()) {
                sink.push(CompilerErrorKind::DuplicateSymbol {
                    first: existing,
                    second: symbol.clone(),
                });
            }
        };

        for env in &parse.wrangler_envs {
            insert_unique(&mut st, sink, &env.symbol);

            for b in &env.d1_bindings {
                insert_unique(&mut st, sink, b);
            }

            for b in &env.r2_bindings {
                insert_unique(&mut st, sink, b);
            }

            for b in &env.kv_bindings {
                insert_unique(&mut st, sink, b);
            }

            for var in &env.vars {
                insert_unique(&mut st, sink, var);
            }
        }

        for model in &parse.models {
            insert_unique(&mut st, sink, &model.symbol);

            for field in &model.fields {
                insert_unique(&mut st, sink, field);
            }
        }

        for api in &parse.apis {
            insert_unique(&mut st, sink, &api.symbol);

            for method in &api.methods {
                insert_unique(&mut st, sink, &method.symbol);

                for param in &method.parameters {
                    insert_unique(&mut st, sink, param);
                }
            }
        }

        for poo in &parse.poos {
            insert_unique(&mut st, sink, &poo.symbol);

            for field in &poo.fields {
                insert_unique(&mut st, sink, field);
            }
        }

        for source in &parse.sources {
            insert_unique(&mut st, sink, &source.symbol);

            for method in [&source.list, &source.get].into_iter().flatten() {
                for param in &method.parameters {
                    insert_unique(&mut st, sink, param);
                }
            }
        }

        for service in &parse.services {
            insert_unique(&mut st, sink, &service.symbol);

            for field in &service.fields {
                insert_unique(&mut st, sink, field);
            }
        }

        for inject in &parse.injects {
            for field in &inject.fields {
                insert_unique(&mut st, sink, field);
            }
        }

        st
    }
}

pub struct SemanticResult {
    pub ast: CloesceAst,
    pub table: SymbolTable,
}

pub struct SemanticAnalysis;
impl SemanticAnalysis {
    pub fn analyze(parse: ParseAst) -> (SemanticResult, Vec<CompilerErrorKind>) {
        let mut sink = ErrorSink::new();

        let mut table = SymbolTable::from_parse(&parse, &mut sink);
        let wrangler_env = Self::wrangler(&parse, &mut sink);
        let mut models = Self::models(&parse, &mut table, &mut sink);
        let poos = Self::poos(&parse, &table, &mut sink);
        let api_map = Self::apis(&parse, &table, &mut sink);
        let data_source_map = Self::data_sources(&parse, &models, &table, &mut sink);
        let services = Self::services(&parse, &table, &mut sink);

        // Merge API methods into their respective models
        for (model_name, api) in api_map {
            if let Some(model) = models.get_mut(&model_name) {
                model.apis.push(api);
            }
        }

        // Merge data sources into their respective models
        for (model_name, ds) in data_source_map {
            if let Some(model) = models.get_mut(&model_name) {
                model.data_sources.push(ds);
            }
        }

        let ast = CloesceAst {
            hash: 0,
            wrangler_env,
            models,
            services,
            poos,
        };

        (SemanticResult { ast, table }, sink.drain())
    }

    fn wrangler(parse: &ParseAst, sink: &mut ErrorSink) -> Option<WranglerEnv> {
        let Some(parsed_env) = parse.wrangler_envs.first() else {
            ensure!(
                parse.models.is_empty(),
                sink,
                CompilerErrorKind::MissingWranglerEnvBlock
            );

            return None;
        };

        Some(WranglerEnv {
            d1_bindings: parsed_env
                .d1_bindings
                .iter()
                .map(|b| b.name.clone())
                .collect(),
            kv_bindings: parsed_env
                .kv_bindings
                .iter()
                .map(|b| b.name.clone())
                .collect(),
            r2_bindings: parsed_env
                .r2_bindings
                .iter()
                .map(|b| b.name.clone())
                .collect(),
            vars: parsed_env
                .vars
                .iter()
                .map(|v| Field {
                    name: v.name.clone(),
                    cidl_type: v.cidl_type.clone(),
                })
                .collect(),
        })
    }

    fn models(
        parse: &ParseAst,
        table: &mut SymbolTable,
        sink: &mut ErrorSink,
    ) -> IndexMap<String, Model> {
        let model_blocks = parse
            .models
            .iter()
            .map(|m| (m.symbol.name.clone(), m))
            .collect::<HashMap<String, &ModelBlock>>();

        match ModelAnalysis::default().analyze(model_blocks, table) {
            Ok(models) => models,
            Err(errs) => {
                sink.extend(errs);
                IndexMap::new()
            }
        }
    }

    fn apis(parse: &ParseAst, table: &SymbolTable, sink: &mut ErrorSink) -> Vec<(String, Api)> {
        match ApiAnalysis::default().analyze(&parse.apis, parse, table) {
            Ok(apis) => apis,
            Err(errs) => {
                sink.extend(errs);
                Vec::new()
            }
        }
    }

    fn data_sources(
        parse: &ParseAst,
        models: &IndexMap<String, Model>,
        table: &SymbolTable,
        sink: &mut ErrorSink,
    ) -> Vec<(String, DataSource)> {
        let mut result = Vec::new();

        // Validate list and get method parameters
        fn analyze_method(
            source_sym: &Symbol,
            method: &frontend::DataSourceBlockMethod,
            sink: &mut ErrorSink,
        ) -> Option<DataSourceMethod> {
            let mut parameters = Vec::new();
            for param in &method.parameters {
                if !is_valid_sql_type(&param.cidl_type) {
                    sink.push(CompilerErrorKind::DataSourceInvalidMethodParam {
                        source: source_sym.clone(),
                        param: param.clone(),
                    });
                }
                parameters.push(Field {
                    name: param.name.clone(),
                    cidl_type: param.cidl_type.clone(),
                });
            }
            Some(DataSourceMethod {
                parameters,
                raw_sql: method.raw_sql.clone(),
            })
        }

        for source in &parse.sources {
            // Validate the model reference
            let Some(model_sym) = table.resolve(&source.model, SymbolKind::ModelDecl, None) else {
                sink.push(CompilerErrorKind::DataSourceUnknownModelReference {
                    source: source.symbol.clone(),
                });
                continue;
            };

            if !matches!(model_sym.kind, SymbolKind::ModelDecl) {
                sink.push(CompilerErrorKind::DataSourceUnknownModelReference {
                    source: source.symbol.clone(),
                });
                continue;
            }

            let model_name = model_sym.name.clone();
            let Some(model) = models.get(&model_name) else {
                sink.push(CompilerErrorKind::DataSourceUnknownModelReference {
                    source: source.symbol.clone(),
                });
                continue;
            };

            // Validate include tree via BFS
            let mut q = VecDeque::new();
            q.push_back((&source.tree, &model_name, model));

            while let Some((node, _parent_model_name, parent_model)) = q.pop_front() {
                for (var_name, child) in &node.0 {
                    // Check navigation properties
                    let nav = parent_model
                        .navigation_fields
                        .iter()
                        .find(|nav| nav.field.name == var_name.as_str());

                    if let Some(nav) = nav {
                        // Navigate into the adjacent model
                        if let Some(adj_model) = models.get(&nav.model_reference) {
                            q.push_back((child, &nav.model_reference, adj_model));
                        }
                        continue;
                    }

                    // Check KV properties
                    if parent_model
                        .kv_fields
                        .iter()
                        .any(|kv| kv.name == var_name.as_str())
                    {
                        continue;
                    }

                    // Check R2 properties
                    if parent_model
                        .r2_fields
                        .iter()
                        .any(|r2| r2.name == var_name.as_str())
                    {
                        continue;
                    }

                    sink.push(CompilerErrorKind::DataSourceInvalidIncludeTreeReference {
                        source: source.symbol.clone(),
                        model: source.model.clone(),
                        name: var_name.clone(),
                    });
                }
            }

            let list = source
                .list
                .as_ref()
                .and_then(|m| analyze_method(&source.symbol, m, sink));
            let get = source
                .get
                .as_ref()
                .and_then(|m| analyze_method(&source.symbol, m, sink));

            result.push((
                model_name,
                DataSource {
                    name: source.symbol.name.clone(),
                    tree: IncludeTree(source.tree.0.clone()),
                    list,
                    get,
                    is_private: false, // TODO: figure out parser scheme for privacy
                },
            ));
        }

        result
    }

    fn poos(
        parse: &ParseAst,
        table: &SymbolTable,
        sink: &mut ErrorSink,
    ) -> BTreeMap<String, PlainOldObject> {
        let mut poos = BTreeMap::new();

        // Cycle detection
        let mut in_degree = BTreeMap::<String, usize>::new();
        let mut graph = BTreeMap::<String, Vec<String>>::new();

        for poo in &parse.poos {
            let poo_name = poo.symbol.name.clone();
            let mut fields = Vec::new();
            graph.entry(poo_name.clone()).or_default();
            in_degree.entry(poo_name.clone()).or_insert(0);

            for field in &poo.fields {
                match field.cidl_type.root_type() {
                    CidlType::Object { name, .. }
                    | CidlType::Partial {
                        object_name: name, ..
                    } => {
                        let Some(ref_sym) = table
                            .resolve(name, SymbolKind::PlainOldObjectDecl, None)
                            .or_else(|| table.resolve(name, SymbolKind::ModelDecl, None))
                        else {
                            sink.push(CompilerErrorKind::UnresolvedSymbol {
                                span: field.span.clone(),
                            });
                            continue;
                        };

                        ensure!(
                            matches!(
                                ref_sym.kind,
                                SymbolKind::PlainOldObjectDecl | SymbolKind::ModelDecl
                            ),
                            sink,
                            CompilerErrorKind::PlainOldObjectInvalidFieldType {
                                field: field.clone(),
                            }
                        );

                        if matches!(ref_sym.kind, SymbolKind::PlainOldObjectDecl) {
                            graph
                                .entry(name.clone())
                                .or_default()
                                .push(poo_name.clone());
                            in_degree.entry(poo_name.clone()).and_modify(|d| *d += 1);
                        }
                    }
                    CidlType::Stream | CidlType::Void => {
                        sink.push(CompilerErrorKind::PlainOldObjectInvalidFieldType {
                            field: field.clone(),
                        });
                    }
                    _ => {
                        // All other types are valid
                    }
                }

                fields.push(Field {
                    name: field.name.clone(),
                    cidl_type: field.cidl_type.clone(),
                });
            }

            poos.insert(
                poo_name.clone(),
                PlainOldObject {
                    name: poo_name,
                    fields,
                },
            );
        }

        match kahns(graph, in_degree, parse.poos.len()) {
            Ok(_) => poos,
            Err(err) => {
                sink.push(err);
                BTreeMap::new()
            }
        }
    }

    fn services(
        parse: &ParseAst,
        table: &SymbolTable,
        sink: &mut ErrorSink,
    ) -> IndexMap<String, Service> {
        let mut services = IndexMap::new();

        // Cycle detection via Kahn's
        let mut in_degree = BTreeMap::<String, usize>::new();
        let mut graph = BTreeMap::<String, Vec<String>>::new();

        for service in &parse.services {
            let service_name = service.symbol.name.clone();
            let mut fields = Vec::new();
            graph.entry(service_name.clone()).or_default();
            in_degree.entry(service_name.clone()).or_insert(0);

            for field in &service.fields {
                match field.cidl_type.root_type() {
                    CidlType::Object { name: ref_name, .. } => {
                        // Try to resolve as inject first, then service
                        let target_sym = table
                            .resolve(ref_name, SymbolKind::InjectDecl, None)
                            .or_else(|| table.resolve(ref_name, SymbolKind::ServiceDecl, None));

                        let Some(target_sym) = target_sym else {
                            sink.push(CompilerErrorKind::ServiceInvalidFieldType {
                                field: field.clone(),
                            });
                            continue;
                        };

                        match target_sym.kind {
                            SymbolKind::InjectDecl => {
                                fields.push(ServiceField {
                                    name: field.name.clone(),
                                    inject_reference: ref_name.clone(),
                                });
                            }
                            SymbolKind::ServiceDecl => {
                                graph
                                    .entry(ref_name.clone())
                                    .or_default()
                                    .push(service_name.clone());
                                *in_degree.entry(service_name.clone()).or_insert(0) += 1;
                                fields.push(ServiceField {
                                    name: field.name.clone(),
                                    inject_reference: ref_name.clone(),
                                });
                            }
                            _ => {
                                sink.push(CompilerErrorKind::ServiceInvalidFieldType {
                                    field: field.clone(),
                                });
                            }
                        }
                    }
                    _ => {
                        sink.push(CompilerErrorKind::ServiceInvalidFieldType {
                            field: field.clone(),
                        });
                    }
                }
            }

            services.insert(
                service_name.clone(),
                Service {
                    name: service_name,
                    fields,
                    apis: Vec::new(),
                },
            );
        }

        match kahns(graph, in_degree, parse.services.len()) {
            Ok(_) => services,
            Err(err) => {
                sink.push(err);
                IndexMap::new()
            }
        }
    }
}

/// Returns if a column in a D1 model is a valid SQLite type
pub fn is_valid_sql_type(cidl_type: &CidlType) -> bool {
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

type AdjacencyList = BTreeMap<String, Vec<String>>;

// Kahns algorithm for topological sort + cycle detection.
// If no cycles, returns a map of name to position used for sorting the original collection.
pub fn kahns(
    graph: AdjacencyList,
    mut in_degree: BTreeMap<String, usize>,
    len: usize,
) -> Result<HashMap<String, usize>, CompilerErrorKind> {
    let mut queue = in_degree
        .iter()
        .filter_map(|(name, &deg)| (deg == 0).then_some(name.clone()))
        .collect::<VecDeque<_>>();

    let mut rank = HashMap::with_capacity(len);
    let mut counter = 0usize;

    while let Some(name) = queue.pop_front() {
        rank.insert(name.clone(), counter);
        counter += 1;

        if let Some(adjs) = graph.get(&name) {
            for adj in adjs {
                let deg = in_degree.get_mut(adj).expect("names to be validated");
                *deg -= 1;

                if *deg == 0 {
                    queue.push_back(adj.clone());
                }
            }
        }
    }

    if rank.len() != len {
        let cycle: Vec<String> = in_degree
            .iter()
            .filter_map(|(n, &d)| (d > 0).then_some(n.clone()))
            .collect();

        if !cycle.is_empty() {
            return Err(CompilerErrorKind::CyclicalRelationship { cycle });
        }
    }

    Ok(rank)
}
