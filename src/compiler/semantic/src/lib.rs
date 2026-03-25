use ast::{
    CidlType, CloesceAst, FileSpan, Model, Api, PlainOldObject, Service, Symbol, SymbolKind,
    SymbolRef, SymbolTable, WranglerEnv, WranglerEnvBindingKind, WranglerSpec,
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
        let services = Self::services(&parse, &table, &mut sink);

        // Merge API methods into their respective models
        for (model_ref, api) in api_map {
            if let Some(model) = models.get_mut(&model_ref) {
                model.apis.push(api);
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

        for env in &parse.wrangler_envs {
            let new_span = FileSpan {
                start: env.span.start,
                end: env.span.end,
                file: env.file.clone(),
            };
            let symbol = Symbol {
                id: env.id,
                name: String::default(),
                span: new_span.clone(),
                kind: SymbolKind::WranglerEnvDecl,
                ..Default::default()
            };

            if let Some(existing) = table.insert(symbol) {
                let first_span = existing.span.clone();
                sink.push(CompilerErrorKind::DuplicateSymbol {
                    symbol: env.id,
                    first_span,
                    second_span: new_span,
                });
            }

            let bindings = env
                .d1_bindings
                .iter()
                .map(|b| (b, WranglerEnvBindingKind::D1))
                .chain(
                    env.kv_bindings
                        .iter()
                        .map(|b| (b, WranglerEnvBindingKind::KV)),
                )
                .chain(
                    env.r2_bindings
                        .iter()
                        .map(|b| (b, WranglerEnvBindingKind::R2)),
                );

            for binding in bindings {
                let new_span = FileSpan {
                    start: binding.0.span.start,
                    end: binding.0.span.end,
                    file: env.file.clone(),
                };
                let symbol = Symbol {
                    id: binding.0.id,
                    name: binding.0.name.clone(),
                    span: new_span.clone(),
                    kind: SymbolKind::WranglerEnvBinding {
                        kind: binding.1.clone(),
                    },
                    ..Default::default()
                };

                if let Some(existing) = table.insert(symbol) {
                    let first_span = existing.span.clone();
                    sink.push(CompilerErrorKind::DuplicateSymbol {
                        symbol: binding.0.id,
                        first_span,
                        second_span: new_span,
                    });
                }
            }

            for var in &env.vars {
                let new_span = FileSpan {
                    start: var.span.start,
                    end: var.span.end,
                    file: env.file.clone(),
                };
                let symbol = Symbol {
                    id: var.id,
                    name: var.name.clone(),
                    span: new_span.clone(),
                    kind: SymbolKind::WranglerEnvVar,
                    ..Default::default()
                };

                if let Some(existing) = table.insert(symbol) {
                    let first_span = existing.span.clone();
                    sink.push(CompilerErrorKind::DuplicateSymbol {
                        symbol: var.id,
                        first_span,
                        second_span: new_span,
                    });
                }
            }
        }

        for model in &parse.models {
            let new_span = FileSpan {
                start: model.span.start,
                end: model.span.end,
                file: model.file.clone(),
            };
            let symbol = Symbol {
                id: model.id,
                name: model.name.clone(),
                span: new_span.clone(),
                kind: SymbolKind::ModelDecl,
                ..Default::default()
            };

            if let Some(existing) = table.insert(symbol) {
                let first_span = existing.span.clone();
                sink.push(CompilerErrorKind::DuplicateSymbol {
                    symbol: model.id,
                    first_span,
                    second_span: new_span,
                });
            }

            if let Some(d1_tag) = &model.d1_binding {
                let symbol = Symbol {
                    id: d1_tag.id,
                    name: String::default(),
                    span: FileSpan {
                        start: d1_tag.span.start,
                        end: d1_tag.span.end,
                        file: model.file.clone(),
                    },
                    kind: SymbolKind::ModelD1Tag,
                    parent: model.id,
                    ..Default::default()
                };
                table.insert(symbol);
            }

            for field in &model.fields {
                let new_span = FileSpan {
                    start: field.span.start,
                    end: field.span.end,
                    file: model.file.clone(),
                };
                let symbol = Symbol {
                    id: field.id,
                    name: field.name.clone(),
                    span: new_span.clone(),
                    kind: SymbolKind::ModelField,
                    parent: model.id,
                    cidl_type: field.cidl_type.clone(),
                };

                if let Some(existing) = table.insert(symbol) {
                    let first_span = existing.span.clone();
                    sink.push(CompilerErrorKind::DuplicateSymbol {
                        symbol: field.id,
                        first_span,
                        second_span: new_span,
                    });
                }
            }

            for fk in &model.foreign_keys {
                let symbol = Symbol {
                    id: fk.id,
                    name: String::default(),
                    span: FileSpan {
                        start: 0,
                        end: 0,
                        file: model.file.clone(),
                    },
                    kind: SymbolKind::ModelForeignKeyTag,
                    parent: model.id,
                    ..Default::default()
                };
                table.insert(symbol);
            }

            for nav in &model.navigation_properties {
                let symbol = Symbol {
                    id: nav.id,
                    name: String::default(),
                    span: FileSpan {
                        start: nav.span.start,
                        end: nav.span.end,
                        file: model.file.clone(),
                    },
                    kind: SymbolKind::ModelNavigationTag,
                    parent: model.id,
                    ..Default::default()
                };
                table.insert(symbol);
            }

            for kv in &model.kvs {
                let symbol = Symbol {
                    id: kv.id,
                    name: String::default(),
                    span: FileSpan {
                        start: kv.span.start,
                        end: kv.span.end,
                        file: model.file.clone(),
                    },
                    kind: SymbolKind::ModelKvTag,
                    parent: model.id,
                    ..Default::default()
                };
                table.insert(symbol);
            }

            for r2 in &model.r2s {
                let symbol = Symbol {
                    id: r2.id,
                    name: String::default(),
                    span: FileSpan {
                        start: r2.span.start,
                        end: r2.span.end,
                        file: model.file.clone(),
                    },
                    kind: SymbolKind::ModelR2Tag,
                    parent: model.id,
                    ..Default::default()
                };
                table.insert(symbol);
            }
        }

        for api in &parse.apis {
            let new_span = FileSpan {
                start: api.span.start,
                end: api.span.end,
                file: api.file.clone(),
            };
            let symbol = Symbol {
                id: api.id,
                name: api.name.clone(),
                span: new_span.clone(),
                kind: SymbolKind::ApiDecl,
                ..Default::default()
            };

            if let Some(existing) = table.insert(symbol) {
                let first_span = existing.span.clone();
                sink.push(CompilerErrorKind::DuplicateSymbol {
                    symbol: api.id,
                    first_span,
                    second_span: new_span,
                });
            }

            for method in &api.methods {
                let new_span = FileSpan {
                    start: method.span.start,
                    end: method.span.end,
                    file: api.file.clone(),
                };
                let symbol = Symbol {
                    id: method.id,
                    name: String::default(),
                    span: new_span.clone(),
                    kind: SymbolKind::ApiMethodDecl,
                    parent: api.id,
                    cidl_type: method.return_type.clone(),
                };

                if let Some(existing) = table.insert(symbol) {
                    let first_span = existing.span.clone();
                    sink.push(CompilerErrorKind::DuplicateSymbol {
                        symbol: method.id,
                        first_span,
                        second_span: new_span,
                    });
                }

                for param in &method.parameters {
                    let new_span = FileSpan {
                        start: param.span.start,
                        end: param.span.end,
                        file: api.file.clone(),
                    };
                    let symbol = Symbol {
                        id: param.id,
                        name: param.name.clone(),
                        span: new_span.clone(),
                        kind: SymbolKind::ApiMethodParam,
                        parent: method.id,
                        cidl_type: param.cidl_type.clone(),
                    };

                    if let Some(existing) = table.insert(symbol) {
                        let first_span = existing.span.clone();
                        sink.push(CompilerErrorKind::DuplicateSymbol {
                            symbol: param.id,
                            first_span,
                            second_span: new_span,
                        });
                    }
                }
            }
        }

        for poo in &parse.poos {
            let new_span = FileSpan {
                start: poo.span.start,
                end: poo.span.end,
                file: poo.file.clone(),
            };
            let symbol = Symbol {
                id: poo.id,
                name: poo.name.clone(),
                span: new_span.clone(),
                kind: SymbolKind::PlainOldObjectDecl,
                ..Default::default()
            };

            if let Some(existing) = table.insert(symbol) {
                let first_span = existing.span.clone();
                sink.push(CompilerErrorKind::DuplicateSymbol {
                    symbol: poo.id,
                    first_span,
                    second_span: new_span,
                });
            }

            for field in &poo.fields {
                let new_span = FileSpan {
                    start: field.span.start,
                    end: field.span.end,
                    file: poo.file.clone(),
                };
                let symbol = Symbol {
                    id: field.id,
                    name: field.name.clone(),
                    span: new_span.clone(),
                    kind: SymbolKind::PlainOldObjectField,
                    parent: poo.id,
                    cidl_type: field.cidl_type.clone(),
                };

                if let Some(existing) = table.insert(symbol) {
                    let first_span = existing.span.clone();
                    sink.push(CompilerErrorKind::DuplicateSymbol {
                        symbol: field.id,
                        first_span,
                        second_span: new_span,
                    });
                }
            }
        }

        for service in &parse.services {
            let new_span = FileSpan {
                start: service.span.start,
                end: service.span.end,
                file: service.file.clone(),
            };
            let symbol = Symbol {
                id: service.id,
                name: service.name.clone(),
                span: new_span.clone(),
                kind: SymbolKind::ServiceDecl,
                ..Default::default()
            };

            if let Some(existing) = table.insert(symbol) {
                let first_span = existing.span.clone();
                sink.push(CompilerErrorKind::DuplicateSymbol {
                    symbol: service.id,
                    first_span,
                    second_span: new_span,
                });
            }

            for field in &service.fields {
                let new_span = FileSpan {
                    start: field.span.start,
                    end: field.span.end,
                    file: service.file.clone(),
                };
                let symbol = Symbol {
                    id: field.id,
                    name: field.name.clone(),
                    span: new_span.clone(),
                    kind: SymbolKind::ServiceField,
                    parent: service.id,
                    cidl_type: field.cidl_type.clone(),
                };

                if let Some(existing) = table.insert(symbol) {
                    let first_span = existing.span.clone();
                    sink.push(CompilerErrorKind::DuplicateSymbol {
                        symbol: field.id,
                        first_span,
                        second_span: new_span,
                    });
                }
            }
        }

        for inject in &parse.injects {
            for &ref_id in &inject.refs {
                let new_span = FileSpan {
                    start: inject.span.start,
                    end: inject.span.end,
                    file: inject.file.clone(),
                };
                let symbol = Symbol {
                    id: ref_id,
                    name: String::default(),
                    span: new_span.clone(),
                    kind: SymbolKind::InjectDecl,
                    ..Default::default()
                };

                if let Some(existing) = table.insert(symbol) {
                    let first_span = existing.span.clone();
                    sink.push(CompilerErrorKind::DuplicateSymbol {
                        symbol: ref_id,
                        first_span,
                        second_span: new_span,
                    });
                }
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

    fn apis(
        parse: &ParseAst,
        table: &SymbolTable,
        sink: &mut ErrorSink,
    ) -> Vec<(SymbolRef, Api)> {
        match ApiAnalysis::default().analyze(&parse.apis, parse, table) {
            Ok(apis) => apis,
            Err(errs) => {
                sink.extend(errs);
                Vec::new()
            }
        }
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
                        sink.push(CompilerErrorKind::ServiceInvalidFieldType {
                            field: field.id,
                        });
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

