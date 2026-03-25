use ast::{
    CidlType, CloesceAst, D1NavigationProperty, FileSpan, ForeignKey, Model,
    NavigationPropertyKind, Symbol, SymbolKind, SymbolRef, SymbolTable, WranglerEnv,
    WranglerEnvBindingKind, WranglerSpec,
};
use frontend::{ForeignKeyTag, ModelBlock, NavigationTag, ParseAst};

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Not,
};

use crate::err::{BatchResult, CompilerErrorKind, ErrorSink};

pub mod err;

// type AdjacencyList<'a> = BTreeMap<&'a str, Vec<&'a str>>;

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
    ) -> HashMap<SymbolRef, Model> {
        let mut d1_model_blocks = HashMap::<SymbolRef, &ModelBlock>::new();
        for model in &parse.models {
            if model.d1_binding.is_some()
                || model.primary_keys.len() > 0
                || model.navigation_properties.len() > 0
                || model.foreign_keys.len() > 0
            {
                d1_model_blocks.insert(model.id, model);
            }

            if model.kvs.len() > 0 || model.r2s.len() > 0 {
                // Self::kv_r2_models(parse, model, sink);
            }
        }

        if d1_model_blocks.is_empty() {
            return HashMap::new();
        }

        match D1ModelAnalysis::default().analyze(d1_model_blocks, table) {
            Ok(models) => models,
            Err(errs) => {
                sink.extend(errs);
                HashMap::new()
            }
        }
    }
}

#[derive(Default)]
struct D1ModelAnalysis {
    sink: ErrorSink,
    in_degree: BTreeMap<SymbolRef, usize>,
    graph: BTreeMap<SymbolRef, Vec<SymbolRef>>,

    /// Maps a field foreign key reference to the model it is referencing
    /// Ie, Person.dogId => { (Person, dogId): "Dog" }
    model_field_to_adj_model: HashMap<(SymbolRef, SymbolRef), SymbolRef>,
}
impl D1ModelAnalysis {
    fn analyze(
        mut self,
        d1_model_blocks: HashMap<SymbolRef, &ModelBlock>,
        table: &mut SymbolTable,
    ) -> BatchResult<HashMap<SymbolRef, Model>> {
        let mut models = HashMap::new();

        for model_block in d1_model_blocks.values() {
            if let Some(model) = self.model(model_block, d1_model_blocks.clone(), table) {
                models.insert(model.symbol, model);
            }
        }

        self.sink.finish()?;
        Ok(models)
    }

    /// Validates a D1 model, returning an ast [Model]
    fn model(
        &mut self,
        model_block: &ModelBlock,
        d1_model_blocks: HashMap<SymbolRef, &ModelBlock>,
        table: &mut SymbolTable,
    ) -> Option<Model> {
        let Some(d1_binding) = &model_block.d1_binding else {
            self.sink.push(CompilerErrorKind::D1ModelMissingD1Binding {
                model: model_block.id,
            });
            return None;
        };

        let Some(binding_symbol) = table.lookup(d1_binding.env_binding) else {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: d1_binding.env_binding,
            });
            return None;
        };

        if matches!(
            binding_symbol.kind,
            SymbolKind::WranglerEnvBinding {
                kind: WranglerEnvBindingKind::D1
            }
        )
        .not()
        {
            self.sink.push(CompilerErrorKind::D1ModelInvalidD1Binding {
                model: model_block.id,
                tag: d1_binding.id,
            });
            return None;
        };

        // At least one primary key must be defined
        if model_block.primary_keys.is_empty() {
            self.sink.push(CompilerErrorKind::D1ModelMissingPrimaryKey {
                model: model_block.id,
            });
            return None;
        }

        // Validate and collect columns
        let mut columns = HashSet::new();
        let mut primary_key_columns = HashSet::new();
        for field in &model_block.fields {
            if !is_valid_sql_type(&field.cidl_type) {
                continue;
            }

            columns.insert(field.id);

            let is_pk = model_block.primary_keys.iter().any(|id| *id == field.id);
            if is_pk {
                ensure!(
                    !field.cidl_type.is_nullable(),
                    self.sink,
                    CompilerErrorKind::NullablePrimaryKey { column: field.id }
                );

                primary_key_columns.insert(field.id);
            }
        }

        self.graph.entry(model_block.id).or_default();
        self.in_degree.entry(model_block.id).or_insert(0);

        // Validate foreign keys
        let mut foreign_keys = Vec::new();
        let mut fk_columns_seen = HashSet::<SymbolRef>::new();
        for fk in &model_block.foreign_keys {
            let fk_result = self.foreign_key(
                model_block,
                fk,
                &columns,
                &mut fk_columns_seen,
                table,
                &d1_model_blocks,
            );
            if let Some(fk) = fk_result {
                foreign_keys.push(fk);
            }
        }

        // Validate navigation properties
        let mut navigation_properties = Vec::new();
        for nav in &model_block.navigation_properties {
            let nav_result = self.nav(model_block, nav, &columns, table, &d1_model_blocks);

            if let Some(nav) = nav_result {
                navigation_properties.push(nav);
            }
        }

        return Some(Model {
            hash: 0,
            symbol: model_block.id,
            d1_binding: Some(d1_binding.env_binding),
            columns,
            primary_key_columns,
            foreign_keys,
            navigation_properties: vec![],
        });
    }

    /// Validates a foreign key, returning an ast [ForeignKey]
    fn foreign_key(
        &mut self,
        model_block: &ModelBlock,
        fk: &ForeignKeyTag,
        columns: &HashSet<SymbolRef>,
        fk_columns_seen: &mut HashSet<SymbolRef>,
        table: &mut SymbolTable,
        d1_model_blocks: &HashMap<SymbolRef, &ModelBlock>,
    ) -> Option<ForeignKey> {
        if table.lookup(fk.adj_model).is_none() {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: fk.adj_model,
            });
            return None;
        }

        let Some(adj_model) = d1_model_blocks.get(&fk.adj_model) else {
            self.sink
                .push(CompilerErrorKind::ForeignKeyReferencesNonD1Model {
                    tag: fk.id,
                    model: fk.adj_model,
                });
            return None;
        };

        if fk.adj_model == model_block.id {
            self.sink.push(CompilerErrorKind::ForeignKeyReferenceSelf {
                model: model_block.id,
                foreign_key: fk.id,
            });
            return None;
        }

        // Must belong to the same database
        if model_block.d1_binding.as_ref().map(|t| t.env_binding)
            != adj_model.d1_binding.as_ref().map(|t| t.env_binding)
        {
            self.sink
                .push(CompilerErrorKind::ForeignKeyReferencesDifferentDatabase {
                    tag: fk.id,
                    binding: adj_model
                        .d1_binding
                        .as_ref()
                        .map(|t| t.env_binding)
                        .unwrap_or(0),
                });
            return None;
        }

        let first_ref = fk.references.first().unwrap().0;
        let is_nullable = table.lookup(first_ref).and_then(|sym| match &sym.kind {
            SymbolKind::ModelField { cidl_type, .. } => Some(cidl_type.is_nullable()),
            _ => None,
        });

        let mut fk_columns = Vec::new();
        for (field, adj_field) in &fk.references {
            fk_columns.push(*field);

            // Validate the field from this model
            let field_cidl_type = {
                // Field should be a column on this model
                if !columns.contains(field) {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk.id,
                            column: *field,
                        },
                    );
                    continue;
                }

                let Some(field_sym) = table.lookup(*field) else {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk.id,
                            column: *field,
                        },
                    );
                    continue;
                };

                // A column cannot be in multiple foreign keys
                if !fk_columns_seen.insert(*field) {
                    self.sink
                        .push(CompilerErrorKind::ForeignKeyColumnAlreadyInForeignKey {
                            tag: fk.id,
                            column: *field,
                        });
                }

                let SymbolKind::ModelField {
                    cidl_type: field_cidl_type,
                    ..
                } = &field_sym.kind
                else {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk.id,
                            column: *field,
                        },
                    );
                    continue;
                };

                if let Some(is_nullable) = is_nullable {
                    if field_cidl_type.is_nullable() != is_nullable {
                        self.sink
                            .push(CompilerErrorKind::ForeignKeyInconsistentNullability {
                                tag: fk.id,
                                first_column: first_ref,
                                second_column: *field,
                            });
                    }
                }

                field_cidl_type
            };

            // Validate the field from the adjacent model
            let adj_field_cidl_type = {
                let Some(adj_field_sym) = table.lookup(*adj_field) else {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk.id,
                            column: *adj_field,
                        },
                    );
                    continue;
                };

                if !d1_model_blocks.get(&fk.adj_model).is_some() {
                    self.sink
                        .push(CompilerErrorKind::ForeignKeyReferencesNonD1Model {
                            tag: fk.id,
                            model: fk.adj_model,
                        });
                    continue;
                }

                let SymbolKind::ModelField {
                    parent: adj_field_parent,
                    cidl_type: adj_field_cidl_type,
                } = &adj_field_sym.kind
                else {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk.id,
                            column: *adj_field,
                        },
                    );
                    continue;
                };

                if !is_valid_sql_type(adj_field_cidl_type) {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk.id,
                            column: *adj_field,
                        },
                    );
                }

                ensure!(
                    *adj_field_parent == fk.adj_model,
                    self.sink,
                    CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                        tag: fk.id,
                        column: *adj_field,
                    }
                );

                adj_field_cidl_type
            };

            if field_cidl_type.root_type() != adj_field_cidl_type.root_type() {
                self.sink.push(
                    CompilerErrorKind::ForeignKeyReferencesIncompatibleColumnType {
                        tag: fk.id,
                        column: *field,
                        adj_column: *adj_field,
                    },
                );
                continue;
            }

            self.model_field_to_adj_model
                .insert((*field, model_block.id), adj_model.id);

            if !field_cidl_type.is_nullable() {
                // One To One: Person has a Dog ..(sql)=> Person has a fk to Dog
                // Dog must come before Person
                self.graph
                    .entry(fk.adj_model)
                    .or_default()
                    .push(model_block.id);
                *self.in_degree.entry(model_block.id).or_insert(0) += 1;
            }
        }

        Some(ForeignKey {
            adj_model: fk.adj_model,
            columns: fk_columns,
        })
    }

    fn nav(
        &mut self,
        model_block: &ModelBlock,
        nav: &NavigationTag,
        columns: &HashSet<SymbolRef>,
        table: &mut SymbolTable,
        d1_model_blocks: &HashMap<SymbolRef, &ModelBlock>,
    ) -> Option<D1NavigationProperty> {
        let Some(nav_field_sym) = table.lookup(nav.field) else {
            self.sink
                .push(CompilerErrorKind::UnresolvedSymbol { symbol: nav.id });
            return None;
        };

        let SymbolKind::ModelField {
            parent,
            cidl_type: nav_field_cidl_type,
        } = &nav_field_sym.kind
        else {
            self.sink.push(
                CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                    tag: nav.id,
                    field: nav.field,
                },
            );
            return None;
        };

        // The nav property must exist on this model
        if *parent != model_block.id {
            self.sink.push(
                CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                    tag: nav.id,
                    field: nav.field,
                },
            );
            return None;
        }

        let Some(adj_model_sym) = table.lookup(nav.adj_model) else {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: nav.adj_model,
            });

            return None;
        };

        if adj_model_sym.id == model_block.id {
            self.sink
                .push(CompilerErrorKind::NavigationPropertyReferencesSelf {
                    model: model_block.id,
                    tag: nav.id,
                });
            return None;
        }

        let Some(adj_model) = d1_model_blocks.get(&adj_model_sym.id) else {
            self.sink
                .push(CompilerErrorKind::NavigationPropertyReferencesNonD1Model {
                    tag: nav.id,
                    model: adj_model_sym.id,
                });
            return None;
        };

        if adj_model.d1_binding.as_ref().map(|t| t.env_binding)
            != model_block.d1_binding.as_ref().map(|t| t.env_binding)
        {
            self.sink.push(
                CompilerErrorKind::NavigationPropertyReferencesDifferentDatabase {
                    tag: nav.id,
                    binding: adj_model
                        .d1_binding
                        .as_ref()
                        .map(|t| t.env_binding)
                        .unwrap_or(0),
                },
            );
            return None;
        }

        let referenced_fields = nav
            .fields
            .iter()
            .filter_map(|f| {
                let Some(field_sym) = table.lookup(*f) else {
                    self.sink.push(
                        CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                            tag: nav.id,
                            field: *f,
                        },
                    );
                    return None;
                };

                if !columns.contains(&field_sym.id) {
                    self.sink.push(
                        CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                            tag: nav.id,
                            field: *f,
                        },
                    );
                    return None;
                }

                Some(field_sym)
            })
            .collect::<Vec<&Symbol>>();
        if referenced_fields.len() != nav.fields.len() {
            // Some referenced fields were invalid, errors caught above
            return None;
        }

        // Ensure both models belong to the same database
        if model_block.d1_binding.as_ref().map(|t| t.env_binding)
            != adj_model.d1_binding.as_ref().map(|t| t.env_binding)
        {
            self.sink.push(
                CompilerErrorKind::NavigationPropertyReferencesDifferentDatabase {
                    tag: nav.id,
                    binding: adj_model
                        .d1_binding
                        .as_ref()
                        .map(|t| t.env_binding)
                        .unwrap_or(0),
                },
            );
            return None;
        }

        // A nav field must be of cidl type Object, that Object must be the adjacent model OR an array of the adjacent model
        fn unwrap_arr_and_null(cidl_type: &CidlType) -> &CidlType {
            match cidl_type {
                CidlType::Array(inner) => inner.as_ref(),
                CidlType::Nullable(inner) => inner.as_ref(),
                other => other,
            }
        }

        match unwrap_arr_and_null(nav_field_cidl_type) {
            CidlType::Object(symbol_ref) => {
                if *symbol_ref != adj_model_sym.id {
                    self.sink.push(
                        CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                            tag: nav.id,
                            field: nav.field,
                        },
                    );
                    return None;
                }
            }
            _ => {
                self.sink.push(
                    CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                        tag: nav.id,
                        field: nav.field,
                    },
                );
                return None;
            }
        }

        let has_arr = matches!(nav_field_cidl_type, CidlType::Array(_));
        let nav = match (has_arr, nav.is_many_to_many) {
            (false, false) => {
                // One to One navigation property
                // References must be a foreign key to the adjacent model
                let has_matching_fk = model_block.foreign_keys.iter().any(|fk| {
                    compare_vecs_ignoring_order(
                        &fk.references.iter().map(|(field, _)| *field).collect(),
                        &nav.fields,
                    ) && fk.adj_model == adj_model_sym.id
                });

                ensure!(
                    has_matching_fk,
                    self.sink,
                    CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                        tag: nav.id,
                        field: nav.field,
                    }
                );

                D1NavigationProperty {
                    hash: 0,
                    field: nav.field,
                    adj_model: nav.adj_model,
                    kind: NavigationPropertyKind::OneToOne {
                        columns: nav.fields.clone(),
                    },
                }
            }
            (true, false) => {
                // One to Many navigation property
                // References must be a foreign key from the adjacent model to this model
                let has_matching_fk = adj_model.foreign_keys.iter().any(|fk| {
                    compare_vecs_ignoring_order(
                        &fk.references
                            .iter()
                            .map(|(_, adj_field)| *adj_field)
                            .collect(),
                        &nav.fields,
                    ) && fk.adj_model == model_block.id
                });

                ensure!(
                    has_matching_fk,
                    self.sink,
                    CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                        tag: nav.id,
                        field: nav.field,
                    }
                );

                D1NavigationProperty {
                    hash: 0,
                    field: nav.field,
                    adj_model: nav.adj_model,
                    kind: NavigationPropertyKind::OneToMany {
                        columns: nav.fields.clone(),
                    },
                }
            }
            (true, true) => {
                // Many to Many navigation property
                let has_matching_nav = adj_model
                    .navigation_properties
                    .iter()
                    .filter(|adj_nav| {
                        adj_nav.is_many_to_many && adj_nav.adj_model == model_block.id
                    })
                    .collect::<Vec<_>>();

                if has_matching_nav.is_empty() {
                    self.sink
                        .push(CompilerErrorKind::NavigationPropertyMissingReciprocalM2M {
                            tag: nav.id,
                        });
                    return None;
                }

                ensure!(
                    has_matching_nav.len() == 1,
                    self.sink,
                    CompilerErrorKind::NavigationPropertyAmbiguousM2M {
                        tag: nav.id,
                        first_m2m_nav: has_matching_nav[0].id,
                        second_m2m_nav: has_matching_nav[1].id,
                    }
                );

                D1NavigationProperty {
                    hash: 0,
                    field: nav.field,
                    adj_model: nav.adj_model,
                    kind: NavigationPropertyKind::ManyToMany,
                }
            }
            _ => {
                self.sink.push(
                    CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                        tag: nav.id,
                        field: nav.field,
                    },
                );
                return None;
            }
        };

        Some(nav)
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

fn compare_vecs_ignoring_order<T: Ord>(a: &Vec<T>, b: &Vec<T>) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut a_sorted: Vec<&T> = a.into_iter().collect();
    a_sorted.sort();

    let mut b_sorted: Vec<&T> = b.into_iter().collect();
    b_sorted.sort();

    a_sorted == b_sorted
}
