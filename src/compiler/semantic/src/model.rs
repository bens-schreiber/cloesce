use ast::{
    CidlType, ForeignKey, KvProperty, Model, NavigationProperty, NavigationPropertyKind,
    R2Property, Symbol, SymbolKind, SymbolRef, SymbolTable, WranglerEnvBindingKind,
};
use frontend::{ForeignKeyTag, KvR2Tag, ModelBlock, NavigationTag};
use indexmap::IndexMap;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Not,
};

use crate::{
    ensure,
    err::{BatchResult, CompilerErrorKind, ErrorSink},
    kahns,
};

#[derive(Default)]
pub struct ModelAnalysis {
    sink: ErrorSink,
    in_degree: BTreeMap<SymbolRef, usize>,
    graph: BTreeMap<SymbolRef, Vec<SymbolRef>>,

    /// Maps a field foreign key reference to the model it is referencing
    /// Ie, Person.dogId => { (Person, dogId): "Dog" }
    model_field_to_adj_model: HashMap<(SymbolRef, SymbolRef), SymbolRef>,
}
impl ModelAnalysis {
    pub fn analyze(
        mut self,
        model_blocks: HashMap<SymbolRef, &ModelBlock>,
        table: &mut SymbolTable,
    ) -> BatchResult<IndexMap<SymbolRef, Model>> {
        let mut models = IndexMap::new();

        for model_block in model_blocks.values() {
            let mut model = Model::default();
            model.symbol = model_block.id;

            if model_block.d1_binding.is_some()
                || !model_block.foreign_keys.is_empty()
                || !model_block.navigation_properties.is_empty()
                || !model_block.primary_keys.is_empty()
            {
                self.d1_properties(&mut model, model_block, model_blocks.clone(), table);
            }

            if !model_block.kvs.is_empty()
                || !model_block.r2s.is_empty()
                || !model_block.key_fields.is_empty()
            {
                self.kv_r2_properties(&mut model, model_block, table);
            }

            models.insert(model_block.id, model);
        }

        match kahns(self.graph, self.in_degree, model_blocks.len()) {
            Ok(rank) => {
                // Sort models according to topological rank
                models.sort_by_key(|k, _| rank.get(k).unwrap_or(&usize::MAX));
            }
            Err(e) => {
                self.sink.push(e);
            }
        }

        self.sink.finish()?;
        Ok(models)
    }

    /// Validates and sets all D1-related properties of a model
    fn d1_properties(
        &mut self,
        model: &mut Model,
        model_block: &ModelBlock,
        model_blocks: HashMap<SymbolRef, &ModelBlock>,
        table: &mut SymbolTable,
    ) {
        let Some(d1_binding) = &model_block.d1_binding else {
            self.sink.push(CompilerErrorKind::D1ModelMissingD1Binding {
                model: model_block.id,
            });
            return;
        };

        let Some(binding_symbol) = table.lookup(d1_binding.env_binding) else {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: d1_binding.env_binding,
            });
            return;
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
            return;
        };

        // At least one primary key must be defined
        if model_block.primary_keys.is_empty() {
            self.sink.push(CompilerErrorKind::D1ModelMissingPrimaryKey {
                model: model_block.id,
            });
            return;
        }

        // Columns
        let mut columns = HashSet::new();
        let mut primary_key_columns = HashSet::new();
        for field in &model_block.fields {
            if !is_valid_sql_type(&field.cidl_type) {
                continue;
            }

            columns.insert(field.id);

            let is_pk = model_block
                .primary_keys
                .iter()
                .any(|pk| pk.field == field.id);
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

        // Foreign keys
        let mut foreign_keys = Vec::new();
        let mut fk_columns_seen = HashSet::<SymbolRef>::new();
        for fk in &model_block.foreign_keys {
            let fk_result = self.foreign_key(
                model_block,
                fk,
                &columns,
                &mut fk_columns_seen,
                table,
                &model_blocks,
            );
            if let Some(fk) = fk_result {
                foreign_keys.push(fk);
            }
        }

        // Navigation properties
        let mut navigation_properties = Vec::new();
        let mut nav_fields_seen = HashSet::<SymbolRef>::new();
        for nav in &model_block.navigation_properties {
            let nav_result = self.nav(model_block, nav, &mut nav_fields_seen, table, &model_blocks);

            if let Some(nav) = nav_result {
                navigation_properties.push(nav);
            }
        }

        // Unique constraints
        let mut unique_constraints = Vec::new();
        for constraint in &model_block.unique_constraints {
            let mut constraint_columns = Vec::new();
            for column in &constraint.fields {
                if !columns.contains(column) {
                    self.sink.push(
                        CompilerErrorKind::UniqueConstraintReferencesInvalidOrUnknownField {
                            tag: constraint.id,
                            field: *column,
                        },
                    );
                    continue;
                }

                constraint_columns.push(*column);
            }

            unique_constraints.push(constraint_columns);
        }

        model.d1_binding = Some(d1_binding.env_binding);
        model.columns = columns;
        model.primary_key_columns = primary_key_columns;
        model.foreign_keys = foreign_keys;
        model.navigation_properties = navigation_properties;
        model.unique_constraints = unique_constraints;
    }

    /// Validates a foreign key, returning an ast [ForeignKey]
    fn foreign_key(
        &mut self,
        model_block: &ModelBlock,
        fk: &ForeignKeyTag,
        columns: &HashSet<SymbolRef>,
        fk_columns_seen: &mut HashSet<SymbolRef>,
        table: &mut SymbolTable,
        model_blocks: &HashMap<SymbolRef, &ModelBlock>,
    ) -> Option<ForeignKey> {
        if table.lookup(fk.adj_model).is_none() {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: fk.adj_model,
            });
            return None;
        }

        let Some(adj_model) = model_blocks.get(&fk.adj_model) else {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: fk.adj_model,
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
        let Some(first_ref_sym) = table.lookup(first_ref) else {
            self.sink.push(
                CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                    tag: fk.id,
                    column: first_ref,
                },
            );
            return None;
        };
        let is_nullable = first_ref_sym.cidl_type.is_nullable();

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

                if field_sym.cidl_type.is_nullable() != is_nullable {
                    self.sink
                        .push(CompilerErrorKind::ForeignKeyInconsistentNullability {
                            tag: fk.id,
                            first_column: first_ref,
                            second_column: *field,
                        });
                }

                &field_sym.cidl_type
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

                if !is_valid_sql_type(&adj_field_sym.cidl_type) {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk.id,
                            column: *adj_field,
                        },
                    );
                }

                ensure!(
                    adj_field_sym.parent == fk.adj_model,
                    self.sink,
                    CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                        tag: fk.id,
                        column: *adj_field,
                    }
                );

                &adj_field_sym.cidl_type
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
        nav_fields_seen: &mut HashSet<SymbolRef>,
        table: &mut SymbolTable,
        model_blocks: &HashMap<SymbolRef, &ModelBlock>,
    ) -> Option<NavigationProperty> {
        if !nav_fields_seen.insert(nav.field) {
            self.sink.push(
                CompilerErrorKind::NavigationPropertyFieldAlreadyInNavigationProperty {
                    tag: nav.id,
                    field: nav.field,
                },
            );
            return None;
        }

        let Some(nav_field_sym) = table.lookup(nav.field) else {
            self.sink
                .push(CompilerErrorKind::UnresolvedSymbol { symbol: nav.id });
            return None;
        };

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

        let Some(adj_model) = model_blocks.get(&adj_model_sym.id) else {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: nav.adj_model,
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

        match unwrap_arr_and_null(&nav_field_sym.cidl_type) {
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

        let has_arr = matches!(nav_field_sym.cidl_type, CidlType::Array(_));
        let nav = match (has_arr, nav.is_many_to_many) {
            (false, false) => {
                // One to One navigation property
                // References must be a foreign key to the adjacent model
                let has_matching_fk = model_block.foreign_keys.iter().any(|fk| {
                    let found_fk_vec = compare_vecs_ignoring_order(
                        &fk.references
                            .iter()
                            .map(|(_, adj_field)| *adj_field)
                            .collect(),
                        &nav.fields,
                    );

                    let it_matches_ids = fk.adj_model == adj_model_sym.id;

                    found_fk_vec && it_matches_ids
                });

                ensure!(
                    has_matching_fk,
                    self.sink,
                    CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                        tag: nav.id,
                        field: nav.field,
                    }
                );

                NavigationProperty {
                    hash: 0,
                    symbol: nav.id,
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
                        &fk.references.iter().map(|(field, _)| *field).collect(),
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

                NavigationProperty {
                    hash: 0,
                    symbol: nav.id,
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

                NavigationProperty {
                    hash: 0,
                    symbol: nav.id,
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

    /// Validates and sets all KV/R2-related properties of a model
    fn kv_r2_properties(
        &mut self,
        model: &mut Model,
        model_block: &ModelBlock,
        table: &SymbolTable,
    ) {
        // Validates that a KV/R2 tag's env binding exists and is of the correct WranglerEnvBindingKind
        let validate_binding =
            |sink: &mut ErrorSink, tag: &KvR2Tag, expected: WranglerEnvBindingKind| -> bool {
                let Some(binding_sym) = table.lookup(tag.env_binding) else {
                    sink.push(CompilerErrorKind::UnresolvedSymbol {
                        symbol: tag.env_binding,
                    });
                    return false;
                };

                let matches_kind = matches!(
                    (&binding_sym.kind, &expected),
                    (
                        SymbolKind::WranglerEnvBinding {
                            kind: WranglerEnvBindingKind::KV
                        },
                        WranglerEnvBindingKind::KV
                    ) | (
                        SymbolKind::WranglerEnvBinding {
                            kind: WranglerEnvBindingKind::R2
                        },
                        WranglerEnvBindingKind::R2
                    )
                );

                if !matches_kind {
                    let err = match expected {
                        WranglerEnvBindingKind::KV => CompilerErrorKind::KvInvalidBinding {
                            tag: tag.id,
                            binding: tag.env_binding,
                        },
                        WranglerEnvBindingKind::R2 => CompilerErrorKind::R2InvalidBinding {
                            tag: tag.id,
                            binding: tag.env_binding,
                        },
                        _ => unreachable!(),
                    };
                    sink.push(err);
                    return false;
                }

                true
            };

        // Extracts variables from a formatted string, then validates that they
        // correspond to fields on the models that are of valid SQLite types
        let validate_key_format = |sink: &mut ErrorSink, tag_id: SymbolRef, format: &str| -> bool {
            let vars = match extract_braced(format) {
                Ok(vars) => vars,
                Err(reason) => {
                    sink.push(CompilerErrorKind::KvR2InvalidKeyFormat {
                        tag: tag_id,
                        reason,
                    });
                    return false;
                }
            };

            for var in vars {
                // Look through fields for a matching name
                let matching_field = model_block.fields.iter().find(|f| {
                    let field_sym = table.lookup(f.id).unwrap();
                    field_sym.name == var && is_valid_sql_type(&field_sym.cidl_type)
                });

                if matching_field.is_none() {
                    sink.push(CompilerErrorKind::KvR2UnknownKeyVariable {
                        tag: tag_id,
                        variable: var,
                    });
                    return false;
                }
            }

            true
        };

        for kv in &model_block.kvs {
            validate_binding(&mut self.sink, kv, WranglerEnvBindingKind::KV);

            if !validate_key_format(&mut self.sink, kv.id, &kv.format) {
                continue;
            }

            model.kv_properties.push(KvProperty {
                symbol: kv.id,
                field: kv.field,
                env_binding: kv.env_binding,
                format: kv.format.clone(),
            });
        }

        for r2 in &model_block.r2s {
            validate_binding(&mut self.sink, r2, WranglerEnvBindingKind::R2);

            let Some(symbol) = table.lookup(r2.field) else {
                self.sink
                    .push(CompilerErrorKind::UnresolvedSymbol { symbol: r2.field });
                continue;
            };

            if !validate_key_format(&mut self.sink, r2.id, &r2.format) {
                continue;
            }

            if symbol.cidl_type != CidlType::R2Object {
                self.sink.push(CompilerErrorKind::KvR2InvalidField {
                    tag: r2.id,
                    field: r2.field,
                });
                continue;
            }

            model.r2_properties.push(R2Property {
                symbol: r2.id,
                field: r2.field,
                env_binding: r2.env_binding,
                format: r2.format.clone(),
            });
        }

        for key_field in &model_block.key_fields {
            let Some(symbol) = table.lookup(key_field.field) else {
                self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                    symbol: key_field.field,
                });
                continue;
            };

            if symbol.cidl_type != CidlType::String {
                self.sink.push(CompilerErrorKind::KvR2InvalidKeyParam {
                    tag: key_field.id,
                    field: key_field.field,
                });
                continue;
            }

            model.key_fields.insert(key_field.field);
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

/// Extracts braced variables from a format string.
/// e.g, "users/{userId}/posts/{postId}" => ["userId", "postId"].
///
/// Returns an error string if the format string is invalid.
fn extract_braced(s: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut current = None;

    for c in s.chars() {
        match (current.as_mut(), c) {
            (None, '{') => current = Some(String::new()),
            (Some(_), '{') => {
                return Err("nested brace in key".to_string());
            }
            (Some(buf), '}') => {
                out.push(std::mem::take(buf));
                current = None;
            }
            (Some(buf), c) => buf.push(c),
            _ => {}
        }
    }

    if current.is_some() {
        return Err("unclosed brace in key".to_string());
    }

    Ok(out)
}
