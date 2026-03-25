use ast::{
    CloesceAst, FileSpan, Model, Symbol, SymbolKind, SymbolRef, SymbolTable, WranglerEnv,
    WranglerEnvBindingKind, WranglerSpec,
};
use frontend::{ModelBlock, ParseAst};
use indexmap::IndexMap;

use std::collections::{HashMap, HashSet};

use crate::{
    err::{CompilerErrorKind, ErrorSink},
    model::ModelAnalysis,
};

pub mod err;
mod model;

pub struct SemanticAnalysis;
impl SemanticAnalysis {
    pub fn analyze(parse: ParseAst, spec: &WranglerSpec) -> (CloesceAst, Vec<CompilerErrorKind>) {
        let mut sink = ErrorSink::new();

        let mut table = Self::symbol_table(&parse, &mut sink);
        let wrangler_env = Self::wrangler(&parse, spec, &mut sink);
        let models = Self::models(&parse, &mut table, &mut sink);

        let ast = CloesceAst {
            wrangler_env,
            models,
            table,
            ..Default::default()
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
                    kind: SymbolKind::WranglerEnvVar {
                        cidl_type: var.cidl_type.clone(),
                    },
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
                    kind: SymbolKind::ModelD1Tag { parent: model.id },
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
                    kind: SymbolKind::ModelField {
                        parent: model.id,
                        cidl_type: field.cidl_type.clone(),
                    },
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
                    kind: SymbolKind::ModelForeignKeyTag { parent: model.id },
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
                    kind: SymbolKind::ModelNavigationTag { parent: model.id },
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
                    kind: SymbolKind::ModelKvTag { parent: model.id },
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
                    kind: SymbolKind::ModelR2Tag { parent: model.id },
                };
                table.insert(symbol);
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
}
