use ast::{
    ApiMethod, CidlType, CloesceAst, DataSource, DataSourceMethod, Field, IncludeTree, Model,
    PlainOldObject, Service, ServiceField, WranglerEnv,
};
use frontend::{ModelBlock, ParseAst, Symbol, SymbolKind};
use indexmap::IndexMap;

use std::collections::{BTreeMap, HashMap, VecDeque};

use crate::{
    api::ApiAnalysis,
    crud::CrudExpansion,
    data_source::DataSourceExpansion,
    err::{CompilerErrorKind, ErrorSink},
    model::ModelAnalysis,
};

mod api;
mod crud;
mod data_source;
pub mod err;
mod model;

#[derive(Default)]
pub struct SymbolTable {
    table: HashMap<String, Symbol>,
}

impl SymbolTable {
    fn key(&self, symbol: &Symbol) -> String {
        match &symbol.kind {
            // Global
            SymbolKind::ModelDecl => format!("model::{}", symbol.name),
            SymbolKind::PlainOldObjectDecl => format!("poo::{}", symbol.name),
            SymbolKind::ServiceDecl => format!("service::{}", symbol.name),
            SymbolKind::WranglerEnvDecl => format!("wrangler_env"),
            SymbolKind::InjectDecl => format!("inject::{}", symbol.name),

            // Scoped
            SymbolKind::WranglerEnvVar => format!("wrangler_env::var::{}", symbol.name),
            SymbolKind::WranglerEnvBinding { kind } => {
                format!("wrangler_env::{}::{}", kind, symbol.name)
            }
            SymbolKind::ModelField => {
                let model_name = &symbol.parent_name;
                format!("model::{model_name}::{}", symbol.name)
            }
            SymbolKind::PlainOldObjectField => {
                let poo_name = &symbol.parent_name;
                format!("poo::{poo_name}::{}", symbol.name)
            }
            SymbolKind::ServiceField => {
                let service_name = &symbol.parent_name;
                format!("service::{service_name}::{}", symbol.name)
            }
            SymbolKind::ApiMethodDecl => {
                let namespace = &symbol.parent_name;
                format!("api::{}::{}", namespace, symbol.name)
            }
            SymbolKind::ApiMethodParam => {
                let namespace_method = &symbol.parent_name; // namespace::method
                format!("api::{namespace_method}::{}", symbol.name)
            }
            SymbolKind::DataSourceDecl => {
                let model = &symbol.parent_name;
                format!("datasource::{model}::{}", symbol.name)
            }
            SymbolKind::DataSourceMethodParam => {
                // model::datasource::method
                // where method is "get" or "list"
                let model_datasource_method = &symbol.parent_name;
                format!("datasource::{model_datasource_method}::{}", symbol.name)
            }

            _ => panic!(
                "unexpected symbol kind in key generation: {:?}",
                symbol.kind
            ),
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

        self.table.get(&key)
    }

    /// Creates a [SymbolTable] from a [ParseAst], catching [CompilerErrorKind::DuplicateSymbol]'s
    fn from_parse(parse: &ParseAst, sink: &mut ErrorSink) -> SymbolTable {
        let mut st = SymbolTable::default();
        let mut global_names = HashMap::new();

        let mut insert_global_name = |sink: &mut ErrorSink, symbol: &Symbol| {
            if let Some(existing) = global_names.insert(symbol.name.to_string(), symbol.clone()) {
                sink.push(CompilerErrorKind::DuplicateSymbol {
                    first: existing,
                    second: symbol.clone(),
                });

                return false;
            }

            true
        };

        let insert_symbol = |st: &mut SymbolTable, sink: &mut ErrorSink, symbol: &Symbol| {
            if let Some(existing) = st.table.insert(st.key(&symbol), symbol.clone()) {
                sink.push(CompilerErrorKind::DuplicateSymbol {
                    first: existing,
                    second: symbol.clone(),
                });

                return false;
            }

            true
        };

        for env in &parse.wrangler_envs {
            insert_symbol(&mut st, sink, &env.symbol);

            for b in &env.d1_bindings {
                if insert_symbol(&mut st, sink, b) {
                    insert_global_name(sink, b);
                }
            }

            for b in &env.r2_bindings {
                if insert_symbol(&mut st, sink, b) {
                    insert_global_name(sink, b);
                }
            }

            for b in &env.kv_bindings {
                if insert_symbol(&mut st, sink, b) {
                    insert_global_name(sink, b);
                }
            }

            for var in &env.vars {
                if insert_symbol(&mut st, sink, var) {
                    insert_global_name(sink, var);
                }
            }
        }

        for model in &parse.models {
            if insert_symbol(&mut st, sink, &model.symbol) {
                insert_global_name(sink, &model.symbol);
            }

            for field in &model.fields {
                insert_symbol(&mut st, sink, field);
            }
        }

        for api in &parse.apis {
            for method in &api.methods {
                insert_symbol(&mut st, sink, &method.symbol);

                for param in &method.parameters {
                    insert_symbol(&mut st, sink, param);
                }
            }
        }

        for poo in &parse.poos {
            if insert_symbol(&mut st, sink, &poo.symbol) {
                insert_global_name(sink, &poo.symbol);
            }

            for field in &poo.fields {
                insert_symbol(&mut st, sink, field);
            }
        }

        for source in &parse.sources {
            if insert_symbol(&mut st, sink, &source.symbol) {
                insert_global_name(sink, &source.symbol);
            }

            for method in [&source.list, &source.get].into_iter().flatten() {
                for param in &method.parameters {
                    insert_symbol(&mut st, sink, param);
                }
            }
        }

        for service in &parse.services {
            if insert_symbol(&mut st, sink, &service.symbol) {
                insert_global_name(sink, &service.symbol);
            }

            for field in &service.fields {
                insert_symbol(&mut st, sink, field);
            }
        }

        for inject in &parse.injects {
            for field in &inject.fields {
                if insert_symbol(&mut st, sink, field) {
                    insert_global_name(sink, field);
                }
            }
        }

        st
    }
}

pub struct SemanticAnalysis;
impl SemanticAnalysis {
    pub fn analyze_with_table(
        parse: ParseAst,
    ) -> ((SymbolTable, CloesceAst), Vec<CompilerErrorKind>) {
        let mut sink = ErrorSink::new();

        let mut table = SymbolTable::from_parse(&parse, &mut sink);
        let wrangler_env = Self::wrangler(&parse, &mut sink);
        let mut models = Self::models(&parse, &mut table, &mut sink);
        let poos = Self::poos(&parse, &table, &mut sink);
        let api_map = Self::apis(&parse, &table, &mut sink);
        let data_source_map = Self::data_sources(&parse, &models, &table, &mut sink);
        let mut services = Self::services(&parse, &table, &mut sink);

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
                model.data_sources.push(ds);
            }
        }

        let mut ast = CloesceAst {
            hash: 0,
            wrangler_env,
            models,
            services,
            poos,
        };
        let errs = sink.drain();
        if !errs.is_empty() {
            return ((table, ast), errs);
        }

        DataSourceExpansion::expand(&mut ast);
        CrudExpansion::expand(&mut ast);
        ((table, ast), vec![])
    }

    pub fn analyze(parse: ParseAst) -> (CloesceAst, Vec<CompilerErrorKind>) {
        let ((_, ast), errors) = Self::analyze_with_table(parse);
        (ast, errors)
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

    fn apis(
        parse: &ParseAst,
        table: &SymbolTable,
        sink: &mut ErrorSink,
    ) -> Vec<(String, Vec<ApiMethod>)> {
        match ApiAnalysis::default().analyze(parse, table) {
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
                        .any(|kv| kv.field.name == var_name.as_str())
                    {
                        continue;
                    }

                    // Check R2 properties
                    if parent_model
                        .r2_fields
                        .iter()
                        .any(|r2| r2.field.name == var_name.as_str())
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
                let resolved_type = match resolve_cidl_type(&field, &field.cidl_type, table) {
                    Ok(t) => t,
                    Err(err) => {
                        sink.push(err);
                        continue;
                    }
                };

                match resolved_type.root_type() {
                    CidlType::Object { name, .. } => {
                        if let Some(ref_sym) =
                            table.resolve(name, SymbolKind::PlainOldObjectDecl, None)
                        {
                            if matches!(ref_sym.kind, SymbolKind::PlainOldObjectDecl)
                                && !field.cidl_type.is_nullable()
                            {
                                graph
                                    .entry(name.clone())
                                    .or_default()
                                    .push(poo_name.clone());
                                in_degree.entry(poo_name.clone()).and_modify(|d| *d += 1);
                            }
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
                    cidl_type: resolved_type,
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
                let resolved_type = match resolve_cidl_type(&field, &field.cidl_type, table) {
                    Ok(t) => t,
                    Err(err) => {
                        sink.push(err);
                        continue;
                    }
                };

                let inject_reference = match resolved_type {
                    CidlType::Inject { name } => {
                        // Only injected fields are allowed in a service
                        name
                    }
                    _ => {
                        sink.push(CompilerErrorKind::ServiceInvalidFieldType {
                            field: field.clone(),
                        });
                        continue;
                    }
                };

                fields.push(ServiceField {
                    name: field.name.clone(),
                    inject_reference,
                });
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

/// Converts a [CidlType::UnresolvedReference] to a resolved type of [CidlType::Object] or [CidlType::Inject]
/// if possible, recursively. Also validates [CidlType::DataSource] and [CidlType::Partial] references.
///
/// Returns an error if the type cannot be resolved or is invalid.
pub(crate) fn resolve_cidl_type(
    symbol: &Symbol,
    cidl_type: &CidlType,
    table: &SymbolTable,
) -> Result<CidlType, CompilerErrorKind> {
    match cidl_type {
        CidlType::UnresolvedReference { name } => {
            if let Some(sym) = table.resolve(&name, SymbolKind::ModelDecl, None) {
                return Ok(CidlType::Object {
                    name: sym.name.clone(),
                });
            }

            if let Some(sym) = table.resolve(&name, SymbolKind::PlainOldObjectDecl, None) {
                return Ok(CidlType::Object {
                    name: sym.name.clone(),
                });
            }

            if let Some(sym) = table.resolve(&name, SymbolKind::ServiceDecl, None) {
                return Ok(CidlType::Inject {
                    name: sym.name.clone(),
                });
            }

            if let Some(sym) = table.resolve(&name, SymbolKind::InjectDecl, None) {
                return Ok(CidlType::Inject {
                    name: sym.name.clone(),
                });
            }

            return Err(CompilerErrorKind::UnresolvedSymbol {
                span: symbol.span.clone(),
            });
        }
        CidlType::DataSource { model_name } => {
            let valid = table
                .resolve(model_name, SymbolKind::ModelDecl, None)
                .is_some();
            if !valid {
                return Err(CompilerErrorKind::UnresolvedSymbol {
                    span: symbol.span.clone(),
                });
            }
            return Ok(cidl_type.clone());
        }
        CidlType::Partial { object_name } => {
            let valid = table
                .resolve(object_name, SymbolKind::PlainOldObjectDecl, None)
                .is_some()
                || table
                    .resolve(object_name, SymbolKind::ModelDecl, None)
                    .is_some();

            if !valid {
                return Err(CompilerErrorKind::UnresolvedSymbol {
                    span: symbol.span.clone(),
                });
            }
            return Ok(cidl_type.clone());
        }
        CidlType::Nullable(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            return Ok(CidlType::Nullable(Box::new(resolved_inner)));
        }
        CidlType::Array(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            return Ok(CidlType::Array(Box::new(resolved_inner)));
        }
        CidlType::Paginated(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            return Ok(CidlType::Paginated(Box::new(resolved_inner)));
        }
        CidlType::KvObject(inner) => {
            let resolved_inner = resolve_cidl_type(symbol, inner, table)?;
            return Ok(CidlType::KvObject(Box::new(resolved_inner)));
        }
        _ => Ok(cidl_type.clone()),
    }
}

/// Returns if a column in a D1 model is a valid SQLite type
pub(crate) fn is_valid_sql_type(cidl_type: &CidlType) -> bool {
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
pub(crate) fn kahns(
    graph: BTreeMap<String, Vec<String>>,
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
