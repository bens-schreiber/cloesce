use ast::{CidlType, CloesceAst, Field, PlainOldObject, Service, ServiceField, WranglerEnv};
use frontend::{ModelBlock, ParseAst, PlainOldObjectBlock, ServiceBlock, Symbol, SymbolKind};
use indexmap::IndexMap;

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, VecDeque},
};

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
pub mod fmt;
mod model;

#[derive(Default)]
pub struct SymbolTable<'src, 'p> {
    table: HashMap<String, &'p Symbol<'src>>,
}

impl<'src, 'p> SymbolTable<'src, 'p> {
    fn key(&self, symbol: &Symbol<'src>) -> String {
        match &symbol.kind {
            // Global
            SymbolKind::ModelDecl => format!("model::{}", symbol.name),
            SymbolKind::PlainOldObjectDecl => format!("poo::{}", symbol.name),
            SymbolKind::ServiceDecl => format!("service::{}", symbol.name),
            SymbolKind::WranglerEnvDecl => "wrangler_env".into(),
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
    fn resolve(
        &self,
        name: &str,
        kind: SymbolKind,
        parent_name: Option<&str>,
    ) -> Option<&'p Symbol<'src>> {
        let key = self.key(&Symbol {
            name,
            kind: kind.clone(),
            parent_name: Cow::Borrowed(parent_name.unwrap_or("")),
            ..Default::default()
        });

        self.table.get(&key).copied()
    }

    /// Creates a [SymbolTable] from a [ParseAst], catching [CompilerErrorKind::DuplicateSymbol]'s
    fn from_parse(
        parse: &'p ParseAst<'src>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> SymbolTable<'src, 'p> {
        let mut st = SymbolTable::default();
        let mut global_names = HashMap::new();

        let mut insert_global_name = |sink: &mut ErrorSink<'src, 'p>, symbol: &'p Symbol<'src>| {
            if let Some(existing) = global_names.insert(symbol.name, symbol) {
                sink.push(SemanticError::DuplicateSymbol {
                    first: existing,
                    second: symbol,
                });

                return false;
            }

            true
        };

        let insert_symbol = |st: &mut SymbolTable<'src, 'p>,
                             sink: &mut ErrorSink<'src, 'p>,
                             symbol: &'p Symbol<'src>| {
            if let Some(existing) = st.table.insert(st.key(symbol), symbol) {
                sink.push(SemanticError::DuplicateSymbol {
                    first: existing,
                    second: symbol,
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
impl<'src, 'p> SemanticAnalysis {
    pub fn analyze(parse: &'p ParseAst<'src>) -> (CloesceAst<'src>, Vec<SemanticError<'src, 'p>>) {
        let mut sink = ErrorSink::new();
        let mut table = SymbolTable::from_parse(parse, &mut sink);

        let wrangler_env = Self::wrangler(parse, &mut sink);

        let mut models = {
            let model_blocks = parse
                .models
                .iter()
                .map(|m| (m.symbol.name, m))
                .collect::<BTreeMap<&str, &ModelBlock>>();

            match ModelAnalysis::default().analyze(model_blocks, &mut table) {
                Ok(models) => models,
                Err(errs) => {
                    sink.extend(errs);
                    IndexMap::new()
                }
            }
        };

        let data_source_map =
            DataSourceAnalysis::analyze(&parse.sources, &models, &table, &mut sink);

        let poos = Self::poos(&parse.poos, &table, &mut sink);

        let api_map = match ApiAnalysis::default().analyze(parse, &table) {
            Ok(apis) => apis,
            Err(errs) => {
                sink.extend(errs);
                Vec::new()
            }
        };

        let mut services = Self::services(&parse.services, &table, &mut sink);

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

        let mut ast = CloesceAst {
            hash: 0,
            wrangler_env,
            models,
            services,
            poos,
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
        parse: &'p ParseAst<'src>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<WranglerEnv<'src>> {
        let Some(parsed_env) = parse.wrangler_envs.first() else {
            ensure!(
                parse.models.is_empty(),
                sink,
                SemanticError::MissingWranglerEnvBlock
            );

            return None;
        };

        Some(WranglerEnv {
            d1_bindings: parsed_env.d1_bindings.iter().map(|b| b.name).collect(),
            kv_bindings: parsed_env.kv_bindings.iter().map(|b| b.name).collect(),
            r2_bindings: parsed_env.r2_bindings.iter().map(|b| b.name).collect(),
            vars: parsed_env
                .vars
                .iter()
                .map(|v| Field {
                    name: v.name.into(),
                    cidl_type: v.cidl_type.clone(),
                })
                .collect(),
        })
    }

    fn poos(
        poo_blocks: &'p [PlainOldObjectBlock<'src>],
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> BTreeMap<&'src str, PlainOldObject<'src>> {
        let mut res = BTreeMap::new();

        // Cycle detection
        let mut in_degree = BTreeMap::<&str, usize>::new();
        let mut graph = BTreeMap::<&str, Vec<&str>>::new();

        for poo in poo_blocks {
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
                    CidlType::Object { name, .. } => {
                        if let Some(ref_sym) =
                            table.resolve(name, SymbolKind::PlainOldObjectDecl, None)
                            && (matches!(ref_sym.kind, SymbolKind::PlainOldObjectDecl)
                                && !field.cidl_type.is_nullable())
                        {
                            graph.entry(name).or_default().push(poo_name);
                            in_degree.entry(poo_name).and_modify(|d| *d += 1);
                        }
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

        match kahns(graph, in_degree, poo_blocks.len()) {
            Ok(_) => res,
            Err(err) => {
                sink.push(err);
                BTreeMap::new()
            }
        }
    }

    fn services(
        service_blocks: &'p [ServiceBlock<'src>],
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> IndexMap<&'src str, Service<'src>> {
        let mut res = IndexMap::new();

        // Cycle detection via Kahn's
        let mut in_degree = BTreeMap::<&str, usize>::new();
        let mut graph = BTreeMap::<&str, Vec<&str>>::new();

        for service in service_blocks {
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

                let inject_reference = match resolved_type {
                    CidlType::Inject { name } => {
                        // Only injected fields are allowed in a service
                        name
                    }
                    _ => {
                        sink.push(SemanticError::ServiceInvalidFieldType { field });
                        continue;
                    }
                };

                fields.push(ServiceField {
                    name: field.name,
                    inject_reference,
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

        match kahns(graph, in_degree, service_blocks.len()) {
            Ok(_) => res,
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
    symbol: &Symbol,
    cidl_type: &CidlType<'src>,
    table: &SymbolTable<'src, 'p>,
) -> Result<CidlType<'src>, SemanticError<'src, 'p>> {
    match cidl_type {
        CidlType::UnresolvedReference { name } => {
            if let Some(sym) = table.resolve(name, SymbolKind::ModelDecl, None) {
                return Ok(CidlType::Object { name: sym.name });
            }

            if let Some(sym) = table.resolve(name, SymbolKind::PlainOldObjectDecl, None) {
                return Ok(CidlType::Object { name: sym.name });
            }

            if let Some(sym) = table.resolve(name, SymbolKind::ServiceDecl, None) {
                return Ok(CidlType::Inject { name: sym.name });
            }

            if let Some(sym) = table.resolve(name, SymbolKind::InjectDecl, None) {
                return Ok(CidlType::Inject { name: sym.name });
            }

            Err(SemanticError::UnresolvedSymbol { span: symbol.span })
        }
        CidlType::DataSource { model_name } => {
            let valid = table
                .resolve(model_name, SymbolKind::ModelDecl, None)
                .is_some();
            if !valid {
                return Err(SemanticError::UnresolvedSymbol { span: symbol.span });
            }
            Ok(cidl_type.clone())
        }
        CidlType::Partial { object_name } => {
            let valid = table
                .resolve(object_name, SymbolKind::PlainOldObjectDecl, None)
                .is_some()
                || table
                    .resolve(object_name, SymbolKind::ModelDecl, None)
                    .is_some();

            if !valid {
                return Err(SemanticError::UnresolvedSymbol { span: symbol.span });
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
