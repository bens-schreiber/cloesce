use ast::{
    Api, CidlType, CloesceAst, DataSource, DataSourceMethod, FileSpan, IncludeTree, Model,
    PlainOldObject, Service, Symbol, SymbolKind, SymbolRef, SymbolTable, WranglerEnv,
    WranglerEnvBindingKind, WranglerSpec,
};
use frontend::{ModelBlock, ParseAst};
use indexmap::IndexMap;

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use crate::{
    api::ApiAnalysis,
    err::{CompilerErrorKind, ErrorSink},
    model::ModelAnalysis,
};

mod api;
pub mod err;
mod model;

pub struct SemanticAnalysis;
impl SemanticAnalysis {
    pub fn analyze(parse: ParseAst, spec: &WranglerSpec) -> (CloesceAst, Vec<CompilerErrorKind>) {
        let mut sink = ErrorSink::new();

        let mut table = Self::symbol_table(&parse, &mut sink);
        let wrangler_env = Self::wrangler(&parse, spec, &mut sink);
        let mut models = Self::models(&parse, &mut table, &mut sink);
        let poos = Self::poos(&parse, &mut table, &mut sink);
        let api_map = Self::apis(&parse, &table, &mut sink);
        let data_source_map = Self::data_sources(&parse, &models, &table, &mut sink);
        let services = Self::services(&parse, &table, &mut sink);

        // Merge API methods into their respective models
        for (model_ref, api) in api_map {
            if let Some(model) = models.get_mut(&model_ref) {
                model.apis.push(api);
            }
        }

        // Merge data sources into their respective models
        for (model_ref, ds) in data_source_map {
            if let Some(model) = models.get_mut(&model_ref) {
                model.data_sources.push(ds);
            }
        }

        let ast = CloesceAst {
            wrangler_env,
            models,
            services,
            table,
            poos,
        };

        (ast, sink.drain())
    }

    /// Converts all declared [ParseId]s into [Symbol]s,
    /// catching duplicate declaration errors along the way.
    fn symbol_table(parse: &ParseAst, sink: &mut ErrorSink) -> SymbolTable {
        let mut table = SymbolTable::default();

        let span = |start, end, file: &std::path::Path| FileSpan {
            start,
            end,
            file: file.to_path_buf(),
        };

        let mut insert_unique = |table: &mut SymbolTable, symbol: Symbol| {
            let id = symbol.id;
            let new_span = symbol.span.clone();
            if let Some(existing) = table.insert(symbol) {
                sink.push(CompilerErrorKind::DuplicateSymbol {
                    symbol: id,
                    first_span: existing.span.clone(),
                    second_span: new_span,
                });
            }
        };

        for env in &parse.wrangler_envs {
            insert_unique(&mut table, Symbol {
                id: env.id,
                span: span(env.span.start, env.span.end, &env.file),
                kind: SymbolKind::WranglerEnvDecl,
                ..Default::default()
            });

            let bindings = env
                .d1_bindings
                .iter()
                .map(|b| (b, WranglerEnvBindingKind::D1))
                .chain(env.kv_bindings.iter().map(|b| (b, WranglerEnvBindingKind::KV)))
                .chain(env.r2_bindings.iter().map(|b| (b, WranglerEnvBindingKind::R2)));

            for (b, kind) in bindings {
                insert_unique(&mut table, Symbol {
                    id: b.id,
                    name: b.name.clone(),
                    span: span(b.span.start, b.span.end, &env.file),
                    kind: SymbolKind::WranglerEnvBinding { kind },
                    ..Default::default()
                });
            }

            for var in &env.vars {
                insert_unique(&mut table, Symbol {
                    id: var.id,
                    name: var.name.clone(),
                    span: span(var.span.start, var.span.end, &env.file),
                    kind: SymbolKind::WranglerEnvVar,
                    ..Default::default()
                });
            }
        }

        for model in &parse.models {
            insert_unique(&mut table, Symbol {
                id: model.id,
                name: model.name.clone(),
                span: span(model.span.start, model.span.end, &model.file),
                kind: SymbolKind::ModelDecl,
                ..Default::default()
            });

            if let Some(d1_tag) = &model.d1_binding {
                table.insert(Symbol {
                    id: d1_tag.id,
                    span: span(d1_tag.span.start, d1_tag.span.end, &model.file),
                    kind: SymbolKind::ModelD1Tag,
                    parent: model.id,
                    ..Default::default()
                });
            }

            for field in &model.fields {
                insert_unique(&mut table, Symbol {
                    id: field.id,
                    name: field.name.clone(),
                    span: span(field.span.start, field.span.end, &model.file),
                    kind: SymbolKind::ModelField,
                    parent: model.id,
                    cidl_type: field.cidl_type.clone(),
                });
            }

            for fk in &model.foreign_keys {
                table.insert(Symbol {
                    id: fk.id,
                    span: span(0, 0, &model.file),
                    kind: SymbolKind::ModelForeignKeyTag,
                    parent: model.id,
                    ..Default::default()
                });
            }

            for nav in &model.navigation_properties {
                table.insert(Symbol {
                    id: nav.id,
                    span: span(nav.span.start, nav.span.end, &model.file),
                    kind: SymbolKind::ModelNavigationTag,
                    parent: model.id,
                    ..Default::default()
                });
            }

            for kv in &model.kvs {
                table.insert(Symbol {
                    id: kv.id,
                    span: span(kv.span.start, kv.span.end, &model.file),
                    kind: SymbolKind::ModelKvTag,
                    parent: model.id,
                    ..Default::default()
                });
            }

            for r2 in &model.r2s {
                table.insert(Symbol {
                    id: r2.id,
                    span: span(r2.span.start, r2.span.end, &model.file),
                    kind: SymbolKind::ModelR2Tag,
                    parent: model.id,
                    ..Default::default()
                });
            }
        }

        for api in &parse.apis {
            insert_unique(&mut table, Symbol {
                id: api.id,
                name: api.name.clone(),
                span: span(api.span.start, api.span.end, &api.file),
                kind: SymbolKind::ApiDecl,
                ..Default::default()
            });

            for method in &api.methods {
                insert_unique(&mut table, Symbol {
                    id: method.id,
                    span: span(method.span.start, method.span.end, &api.file),
                    kind: SymbolKind::ApiMethodDecl,
                    parent: api.id,
                    cidl_type: method.return_type.clone(),
                    ..Default::default()
                });

                for param in &method.parameters {
                    insert_unique(&mut table, Symbol {
                        id: param.id,
                        name: param.name.clone(),
                        span: span(param.span.start, param.span.end, &api.file),
                        kind: SymbolKind::ApiMethodParam,
                        parent: method.id,
                        cidl_type: param.cidl_type.clone(),
                    });
                }
            }
        }

        for poo in &parse.poos {
            insert_unique(&mut table, Symbol {
                id: poo.id,
                name: poo.name.clone(),
                span: span(poo.span.start, poo.span.end, &poo.file),
                kind: SymbolKind::PlainOldObjectDecl,
                ..Default::default()
            });

            for field in &poo.fields {
                insert_unique(&mut table, Symbol {
                    id: field.id,
                    name: field.name.clone(),
                    span: span(field.span.start, field.span.end, &poo.file),
                    kind: SymbolKind::PlainOldObjectField,
                    parent: poo.id,
                    cidl_type: field.cidl_type.clone(),
                });
            }
        }

        for source in &parse.sources {
            insert_unique(&mut table, Symbol {
                id: source.id,
                name: source.name.clone(),
                span: span(source.span.start, source.span.end, &source.file),
                kind: SymbolKind::DataSourceDecl,
                parent: source.model,
                ..Default::default()
            });

            for method in [&source.list, &source.get].into_iter().flatten() {
                for param in &method.parameters {
                    insert_unique(&mut table, Symbol {
                        id: param.id,
                        name: param.name.clone(),
                        span: span(param.span.start, param.span.end, &source.file),
                        kind: SymbolKind::DataSourceMethodParam,
                        parent: source.id,
                        cidl_type: param.cidl_type.clone(),
                    });
                }
            }
        }

        for service in &parse.services {
            insert_unique(&mut table, Symbol {
                id: service.id,
                name: service.name.clone(),
                span: span(service.span.start, service.span.end, &service.file),
                kind: SymbolKind::ServiceDecl,
                ..Default::default()
            });

            for field in &service.fields {
                insert_unique(&mut table, Symbol {
                    id: field.id,
                    name: field.name.clone(),
                    span: span(field.span.start, field.span.end, &service.file),
                    kind: SymbolKind::ServiceField,
                    parent: service.id,
                    cidl_type: field.cidl_type.clone(),
                });
            }
        }

        for inject in &parse.injects {
            for &ref_id in &inject.refs {
                insert_unique(&mut table, Symbol {
                    id: ref_id,
                    span: span(inject.span.start, inject.span.end, &inject.file),
                    kind: SymbolKind::InjectDecl,
                    ..Default::default()
                });
            }
        }

        table
    }

    /// If multiple environments are declared, sinks an error but returns the first environments bindings.
    /// If no environment is declared, sinks an error if there are any models (since models require an env), but returns None.
    fn wrangler(
        parse: &ParseAst,
        spec: &WranglerSpec,
        sink: &mut ErrorSink,
    ) -> Option<WranglerEnv> {
        ensure!(
            parse.wrangler_envs.len() < 2,
            sink,
            CompilerErrorKind::MultipleWranglerEnvBlocks {
                first: parse.wrangler_envs[0].id,
                second: parse.wrangler_envs[1].id,
            }
        );

        let Some(parsed_env) = parse.wrangler_envs.first() else {
            ensure!(
                parse.models.is_empty(),
                sink,
                CompilerErrorKind::MissingWranglerEnvBlock
            );

            return None;
        };

        let mut vars = HashSet::new();
        let mut d1_bindings = HashSet::new();
        let mut kv_bindings = HashSet::new();
        let mut r2_bindings = HashSet::new();

        for var in &parsed_env.vars {
            ensure!(
                spec.vars.contains_key(var.name.as_str()),
                sink,
                CompilerErrorKind::WranglerBindingInconsistentWithSpec { binding: var.id }
            );

            vars.insert(var.id);
        }

        for db in &parsed_env.d1_bindings {
            ensure!(
                spec.d1_databases
                    .iter()
                    .any(|d| d.binding.as_ref().is_some_and(|b| b == db.name.as_str())),
                sink,
                CompilerErrorKind::WranglerBindingInconsistentWithSpec { binding: db.id }
            );

            d1_bindings.insert(db.id);
        }

        for kv in &parsed_env.kv_bindings {
            ensure!(
                spec.kv_namespaces
                    .iter()
                    .any(|ns| ns.binding.as_ref().is_some_and(|b| b == kv.name.as_str())),
                sink,
                CompilerErrorKind::WranglerBindingInconsistentWithSpec { binding: kv.id }
            );

            kv_bindings.insert(kv.id);
        }

        for r2 in &parsed_env.r2_bindings {
            ensure!(
                spec.r2_buckets
                    .iter()
                    .any(|b| b.binding.as_ref().is_some_and(|b| b == r2.name.as_str())),
                sink,
                CompilerErrorKind::WranglerBindingInconsistentWithSpec { binding: r2.id }
            );

            r2_bindings.insert(r2.id);
        }

        Some(WranglerEnv {
            symbol: parsed_env.id,
            d1_bindings: d1_bindings,
            kv_bindings: kv_bindings,
            r2_bindings: r2_bindings,
            vars: vars,
        })
    }

    fn models(
        parse: &ParseAst,
        table: &mut SymbolTable,
        sink: &mut ErrorSink,
    ) -> IndexMap<SymbolRef, Model> {
        let model_map = parse
            .models
            .iter()
            .map(|m| (m.id, m))
            .collect::<HashMap<SymbolRef, &ModelBlock>>();

        match ModelAnalysis::default().analyze(model_map, table) {
            Ok(models) => models,
            Err(errs) => {
                sink.extend(errs);
                IndexMap::new()
            }
        }
    }

    fn apis(parse: &ParseAst, table: &SymbolTable, sink: &mut ErrorSink) -> Vec<(SymbolRef, Api)> {
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
        models: &IndexMap<SymbolRef, Model>,
        table: &SymbolTable,
        sink: &mut ErrorSink,
    ) -> Vec<(SymbolRef, DataSource)> {
        let mut result = Vec::new();

        // Validate list and get method parameters
        fn analyze_method(
            source: &frontend::DataSourceBlock,
            method: &frontend::DataSourceBlockMethod,
            sink: &mut ErrorSink,
        ) -> Option<DataSourceMethod> {
            let mut params = Vec::new();
            for param in &method.parameters {
                if !is_valid_sql_type(&param.cidl_type) {
                    sink.push(CompilerErrorKind::DataSourceInvalidMethodParam {
                        source: source.id,
                        param: param.id,
                    });
                }
                params.push(param.id);
            }
            Some(DataSourceMethod {
                symbol: source.id,
                parameters: params,
                raw_sql: method.raw_sql.clone(),
            })
        }

        for source in &parse.sources {
            // Validate the model reference
            let Some(model_sym) = table.lookup(source.model) else {
                sink.push(CompilerErrorKind::DataSourceUnknownModelReference { source: source.id });
                continue;
            };

            if !matches!(model_sym.kind, SymbolKind::ModelDecl) {
                sink.push(CompilerErrorKind::DataSourceUnknownModelReference { source: source.id });
                continue;
            }

            let Some(model) = models.get(&source.model) else {
                sink.push(CompilerErrorKind::DataSourceUnknownModelReference { source: source.id });
                continue;
            };

            // Validate include tree via BFS
            let mut q = VecDeque::new();
            q.push_back((&source.tree, source.model, model));

            while let Some((node, parent_model_ref, parent_model)) = q.pop_front() {
                for (var_name, child) in &node.0 {
                    // Check navigation properties
                    let nav = parent_model
                        .navigation_properties
                        .iter()
                        .find(|nav| table.name(nav.field) == var_name.as_str());

                    if let Some(nav) = nav {
                        // Navigate into the adjacent model
                        if let Some(adj_model) = models.get(&nav.adj_model) {
                            q.push_back((child, nav.adj_model, adj_model));
                        }
                        continue;
                    }

                    // Check KV properties
                    if parent_model
                        .kv_properties
                        .iter()
                        .any(|kv| table.name(kv.field) == var_name.as_str())
                    {
                        continue;
                    }

                    // Check R2 properties
                    if parent_model
                        .r2_properties
                        .iter()
                        .any(|r2| table.name(r2.field) == var_name.as_str())
                    {
                        continue;
                    }

                    sink.push(CompilerErrorKind::DataSourceInvalidIncludeTreeReference {
                        source: source.id,
                        model: parent_model_ref,
                        name: var_name.clone(),
                    });
                }
            }

            let list = source
                .list
                .as_ref()
                .and_then(|m| analyze_method(source, m, sink));
            let get = source
                .get
                .as_ref()
                .and_then(|m| analyze_method(source, m, sink));

            result.push((
                source.model,
                DataSource {
                    symbol: source.id,
                    tree: IncludeTree(source.tree.0.clone()),
                    list,
                    get,
                },
            ));
        }

        result
    }

    fn poos(
        parse: &ParseAst,
        table: &mut SymbolTable,
        sink: &mut ErrorSink,
    ) -> HashMap<SymbolRef, PlainOldObject> {
        let mut poos = HashMap::new();

        // Cycle detection
        let mut in_degree = BTreeMap::<SymbolRef, usize>::new();
        let mut graph = BTreeMap::<SymbolRef, Vec<SymbolRef>>::new();

        for poo in &parse.poos {
            let mut fields = HashSet::new();
            graph.entry(poo.id).or_default();
            in_degree.entry(poo.id).or_insert(0);

            for field in &poo.fields {
                let Some(field_sym) = table.lookup(field.id) else {
                    sink.push(CompilerErrorKind::UnresolvedSymbol { symbol: field.id });
                    continue;
                };

                match field_sym.cidl_type.root_type() {
                    // TODO: data sources
                    CidlType::Object(o) | CidlType::Partial(o) => {
                        let Some(poo_sym) = table.lookup(*o) else {
                            sink.push(CompilerErrorKind::UnresolvedSymbol { symbol: *o });
                            continue;
                        };

                        ensure!(
                            matches!(
                                poo_sym.kind,
                                SymbolKind::PlainOldObjectDecl | SymbolKind::ModelDecl
                            ),
                            sink,
                            CompilerErrorKind::PlainOldObjectInvalidFieldType { field: field.id }
                        );

                        if matches!(poo_sym.kind, SymbolKind::PlainOldObjectDecl) {
                            graph.entry(*o).or_default().push(poo.id);
                            in_degree.entry(poo.id).and_modify(|d| *d += 1);
                        }
                    }
                    CidlType::Stream | CidlType::Void => {
                        sink.push(CompilerErrorKind::PlainOldObjectInvalidFieldType {
                            field: field.id,
                        });
                    }
                    _ => {
                        // All other types are valid
                        fields.insert(field.id);
                    }
                }
            }

            poos.insert(
                poo.id,
                PlainOldObject {
                    symbol: poo.id,
                    fields,
                },
            );
        }

        match kahns(graph, in_degree, parse.poos.len()) {
            Ok(_) => poos,
            Err(err) => {
                sink.push(err);
                HashMap::new()
            }
        }
    }

    fn services(
        parse: &ParseAst,
        table: &SymbolTable,
        sink: &mut ErrorSink,
    ) -> IndexMap<SymbolRef, Service> {
        let mut services = IndexMap::new();

        // Cycle detection via Kahn's
        let mut in_degree = BTreeMap::<SymbolRef, usize>::new();
        let mut graph = BTreeMap::<SymbolRef, Vec<SymbolRef>>::new();

        for service in &parse.services {
            let mut fields = HashSet::new();
            graph.entry(service.id).or_default();
            in_degree.entry(service.id).or_insert(0);

            for field in &service.fields {
                let Some(field_sym) = table.lookup(field.id) else {
                    sink.push(CompilerErrorKind::UnresolvedSymbol { symbol: field.id });
                    continue;
                };

                match field_sym.cidl_type.root_type() {
                    CidlType::Object(ref_id) => {
                        let Some(target_sym) = table.lookup(*ref_id) else {
                            sink.push(CompilerErrorKind::UnresolvedSymbol { symbol: *ref_id });
                            continue;
                        };

                        match target_sym.kind {
                            SymbolKind::InjectDecl => {
                                fields.insert(field.id);
                            }
                            SymbolKind::ServiceDecl => {
                                graph.entry(*ref_id).or_default().push(service.id);
                                *in_degree.entry(service.id).or_insert(0) += 1;
                                fields.insert(field.id);
                            }
                            _ => {
                                sink.push(CompilerErrorKind::ServiceInvalidFieldType {
                                    field: field.id,
                                });
                            }
                        }
                    }
                    _ => {
                        sink.push(CompilerErrorKind::ServiceInvalidFieldType { field: field.id });
                    }
                }
            }

            services.insert(
                service.id,
                Service {
                    symbol: service.id,
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

type AdjacencyList = BTreeMap<SymbolRef, Vec<SymbolRef>>;

// Kahns algorithm for topological sort + cycle detection.
// If no cycles, returns a map of id to position used for sorting the original collection.
pub fn kahns(
    graph: AdjacencyList,
    mut in_degree: BTreeMap<SymbolRef, usize>,
    len: usize,
) -> Result<HashMap<SymbolRef, usize>, CompilerErrorKind> {
    let mut queue = in_degree
        .iter()
        .filter_map(|(&id, &deg)| (deg == 0).then_some(id))
        .collect::<VecDeque<_>>();

    let mut rank = HashMap::with_capacity(len);
    let mut counter = 0usize;

    while let Some(id) = queue.pop_front() {
        rank.insert(id, counter);
        counter += 1;

        if let Some(adjs) = graph.get(&id) {
            for adj in adjs {
                let deg = in_degree.get_mut(adj).expect("names to be validated");
                *deg -= 1;

                if *deg == 0 {
                    queue.push_back(*adj);
                }
            }
        }
    }

    if rank.len() != len {
        let cycle: Vec<SymbolRef> = in_degree
            .iter()
            .filter_map(|(&n, &d)| (d > 0).then_some(n))
            .collect();

        if cycle.len() > 0 {
            return Err(CompilerErrorKind::CyclicalRelationship { cycle });
        }
    }

    Ok(rank)
}
