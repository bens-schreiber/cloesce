use ast::{
    CidlType, Column, CrudKind, Field, ForeignKeyReference, KvR2Field, Model, NavigationField,
    NavigationFieldKind,
};
use frontend::{ForeignKeyTag, KvR2Tag, ModelBlock, NavigationTag, parser::ParseId};
use indexmap::IndexMap;

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Not,
};

use crate::{
    Symbol, SymbolKind, SymbolTable, WranglerEnvBindingKind, ensure,
    err::{BatchResult, CompilerErrorKind, ErrorSink},
    is_valid_sql_type, kahns,
};

#[derive(Default)]
pub struct ModelAnalysis {
    sink: ErrorSink,
    in_degree: BTreeMap<ParseId, usize>,
    graph: BTreeMap<ParseId, Vec<ParseId>>,
}

impl ModelAnalysis {
    pub fn analyze(
        mut self,
        model_blocks: HashMap<ParseId, &ModelBlock>,
        table: &mut SymbolTable,
    ) -> BatchResult<IndexMap<String, Model>> {
        let mut models: IndexMap<String, Model> = IndexMap::new();
        // Map from ParseId -> model name for ordering
        let mut id_to_name: HashMap<ParseId, String> = HashMap::new();

        for model_block in model_blocks.values() {
            let model_name = model_block.name.clone();
            id_to_name.insert(model_block.id, model_name.clone());

            let mut model = Model {
                name: model_name.clone(),
                d1_binding: None,
                primary_columns: Vec::new(),
                columns: Vec::new(),
                kv_fields: Vec::new(),
                r2_fields: Vec::new(),
                navigation_properties: Vec::new(),
                key_fields: Vec::new(),
                apis: Vec::new(),
                data_sources: Vec::new(),
                cruds: Vec::new(),
            };

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

            // Validate CRUD
            for crud in &model_block.cruds {
                if matches!(crud, CrudKind::LIST) && model.d1_binding.is_none() {
                    self.sink.push(CompilerErrorKind::UnsupportedCrudOperation {
                        model: model_block.id,
                    });
                }
            }

            model.cruds = model_block.cruds.clone();
            models.insert(model_name, model);
        }

        match kahns(self.graph, self.in_degree, model_blocks.len()) {
            Ok(rank) => {
                // Sort models according to topological rank using id_to_name mapping
                models.sort_by_key(|name, _| {
                    // Find the ParseId for this model name
                    id_to_name
                        .iter()
                        .find(|(_, n)| *n == name)
                        .map(|(id, _)| rank.get(id).unwrap_or(&usize::MAX))
                        .copied()
                        .unwrap_or(usize::MAX)
                });
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
        model_blocks: HashMap<ParseId, &ModelBlock>,
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

        let binding_name = binding_symbol.name.clone();

        // At least one primary key must be defined
        if model_block.primary_keys.is_empty() {
            self.sink.push(CompilerErrorKind::D1ModelMissingPrimaryKey {
                model: model_block.id,
            });
            return;
        }

        // Columns - track ParseId -> field name for FK resolution
        let mut column_ids = HashSet::new();
        let mut primary_column_ids = HashSet::new();

        // Track foreign key info per column: ParseId -> (adj_model_name, adj_column_name, composite_id)
        let mut fk_info: HashMap<ParseId, (String, String, Option<usize>)> = HashMap::new();
        // Track unique constraint membership per column: ParseId -> Vec<usize>
        let mut unique_info: HashMap<ParseId, Vec<usize>> = HashMap::new();

        for field in &model_block.fields {
            if !is_valid_sql_type(&field.cidl_type) {
                continue;
            }
            column_ids.insert(field.id);

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
                primary_column_ids.insert(field.id);
            }
        }

        self.graph.entry(model_block.id).or_default();
        self.in_degree.entry(model_block.id).or_insert(0);

        // Foreign keys
        let mut fk_columns_seen = HashSet::<ParseId>::new();
        let mut composite_counter = 0usize;
        for fk in &model_block.foreign_keys {
            self.foreign_key(
                model_block,
                fk,
                &column_ids,
                &mut fk_columns_seen,
                table,
                &model_blocks,
                &mut fk_info,
                &mut composite_counter,
            );
        }

        // Navigation properties
        let mut navigation_properties = Vec::new();
        let mut nav_fields_seen = HashSet::<ParseId>::new();
        for nav in &model_block.navigation_properties {
            let nav_result = self.nav(model_block, nav, &mut nav_fields_seen, table, &model_blocks);

            if let Some(nav) = nav_result {
                navigation_properties.push(nav);
            }
        }

        // Unique constraints
        for (constraint_idx, constraint) in model_block.unique_constraints.iter().enumerate() {
            for column in &constraint.fields {
                if !column_ids.contains(column) {
                    self.sink.push(
                        CompilerErrorKind::UniqueConstraintReferencesInvalidOrUnknownField {
                            tag: constraint.id,
                            field: *column,
                        },
                    );
                    continue;
                }
                unique_info.entry(*column).or_default().push(constraint_idx);
            }
        }

        // Build Column structs
        let mut primary_columns = Vec::new();
        let mut columns = Vec::new();
        for field in &model_block.fields {
            if !column_ids.contains(&field.id) {
                continue;
            }

            let foreign_key_reference =
                fk_info
                    .get(&field.id)
                    .map(|(model_name, col_name, _)| ForeignKeyReference {
                        model_name: model_name.clone(),
                        column_name: col_name.clone(),
                    });
            let composite_id = fk_info.get(&field.id).and_then(|(_, _, cid)| *cid);
            let unique_ids_val = unique_info.remove(&field.id).unwrap_or_default();

            let col = Column {
                field: Field {
                    name: field.name.clone(),
                    cidl_type: field.cidl_type.clone(),
                },
                foreign_key_reference,
                unique_ids: unique_ids_val,
                composite_id,
            };

            if primary_column_ids.contains(&field.id) {
                primary_columns.push(col);
            } else {
                columns.push(col);
            }
        }

        model.d1_binding = Some(binding_name);
        model.columns = columns;
        model.primary_columns = primary_columns;
        model.navigation_properties = navigation_properties;
    }

    /// Validates a foreign key and populates fk_info map
    fn foreign_key(
        &mut self,
        model_block: &ModelBlock,
        fk: &ForeignKeyTag,
        columns: &HashSet<ParseId>,
        fk_columns_seen: &mut HashSet<ParseId>,
        table: &mut SymbolTable,
        model_blocks: &HashMap<ParseId, &ModelBlock>,
        fk_info: &mut HashMap<ParseId, (String, String, Option<usize>)>,
        composite_counter: &mut usize,
    ) {
        if table.lookup(fk.adj_model).is_none() {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: fk.adj_model,
            });
            return;
        }

        let Some(adj_model) = model_blocks.get(&fk.adj_model) else {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: fk.adj_model,
            });
            return;
        };

        if fk.adj_model == model_block.id {
            self.sink.push(CompilerErrorKind::ForeignKeyReferenceSelf {
                model: model_block.id,
                foreign_key: fk.id,
            });
            return;
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
            return;
        }

        let first_ref = fk.references.first().unwrap().0;
        let Some(first_ref_sym) = table.lookup(first_ref) else {
            self.sink.push(
                CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                    tag: fk.id,
                    column: first_ref,
                },
            );
            return;
        };
        let is_nullable = first_ref_sym.cidl_type.is_nullable();

        let is_composite = fk.references.len() > 1;
        let composite_id = if is_composite {
            let id = *composite_counter;
            *composite_counter += 1;
            Some(id)
        } else {
            None
        };

        let adj_model_name = adj_model.name.clone();

        for (field, adj_field) in &fk.references {
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
            let (adj_field_cidl_type, adj_field_name) = {
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

                (&adj_field_sym.cidl_type, adj_field_sym.name.clone())
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

            // Store FK info for this column
            fk_info.insert(
                *field,
                (adj_model_name.clone(), adj_field_name, composite_id),
            );

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
    }

    fn nav(
        &mut self,
        model_block: &ModelBlock,
        nav: &NavigationTag,
        nav_fields_seen: &mut HashSet<ParseId>,
        table: &mut SymbolTable,
        model_blocks: &HashMap<ParseId, &ModelBlock>,
    ) -> Option<NavigationField> {
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
            CidlType::Object { id, .. } => {
                if *id != adj_model_sym.id {
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

        let adj_model_name = adj_model_sym.name.clone();
        let nav_field = Field {
            name: nav_field_sym.name.clone(),
            cidl_type: nav_field_sym.cidl_type.clone(),
        };

        // Convert referenced field ParseIds to names
        let referenced_field_names: Vec<String> = nav
            .fields
            .iter()
            .filter_map(|f| table.lookup(*f).map(|s| s.name.clone()))
            .collect();

        let has_arr = matches!(nav_field_sym.cidl_type, CidlType::Array(_));
        let nav_result = match (has_arr, nav.is_many_to_many) {
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

                NavigationField {
                    field: nav_field,
                    model_reference: adj_model_name,
                    kind: NavigationFieldKind::OneToOne {
                        columns: referenced_field_names,
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

                NavigationField {
                    field: nav_field,
                    model_reference: adj_model_name,
                    kind: NavigationFieldKind::OneToMany {
                        columns: referenced_field_names,
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

                NavigationField {
                    field: nav_field,
                    model_reference: adj_model_name,
                    kind: NavigationFieldKind::ManyToMany,
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

        Some(nav_result)
    }

    /// Validates and sets all KV/R2-related properties of a model
    fn kv_r2_properties(
        &mut self,
        model: &mut Model,
        model_block: &ModelBlock,
        table: &SymbolTable,
    ) {
        // Validates that a KV/R2 tag's env binding exists and is of the correct WranglerEnvBindingKind
        let validate_binding = |sink: &mut ErrorSink,
                                tag: &KvR2Tag,
                                expected: WranglerEnvBindingKind|
         -> Option<String> {
            let Some(binding_sym) = table.lookup(tag.env_binding) else {
                sink.push(CompilerErrorKind::UnresolvedSymbol {
                    symbol: tag.env_binding,
                });
                return None;
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
                return None;
            }

            Some(binding_sym.name.clone())
        };

        // Extracts variables from a formatted string, then validates that they
        // correspond to fields on the models that are of valid SQLite types
        let validate_key_format = |sink: &mut ErrorSink, tag_id: ParseId, format: &str| -> bool {
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
            let binding_name = validate_binding(&mut self.sink, kv, WranglerEnvBindingKind::KV);

            if !validate_key_format(&mut self.sink, kv.id, &kv.format) {
                continue;
            }

            let Some(field_sym) = table.lookup(kv.field) else {
                self.sink
                    .push(CompilerErrorKind::UnresolvedSymbol { symbol: kv.field });
                continue;
            };

            model.kv_fields.push(KvR2Field {
                name: field_sym.name.clone(),
                cidl_type: field_sym.cidl_type.clone(),
                format: kv.format.clone(),
                binding: binding_name.unwrap_or_default(),
                list_prefix: false,
            });
        }

        for r2 in &model_block.r2s {
            let binding_name = validate_binding(&mut self.sink, r2, WranglerEnvBindingKind::R2);

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

            model.r2_fields.push(KvR2Field {
                name: symbol.name.clone(),
                cidl_type: symbol.cidl_type.clone(),
                format: r2.format.clone(),
                binding: binding_name.unwrap_or_default(),
                list_prefix: false,
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

            model.key_fields.push(symbol.name.clone());
        }
    }
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
