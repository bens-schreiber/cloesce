use ast::{
    CidlType, Column, CrudKind, Field, ForeignKeyReference, KvR2Field, Model, NavigationField,
    NavigationFieldKind,
};
use frontend::{
    FileSpan, ForeignKeyTag, KvR2Tag, ModelBlock, NavigationTag, WranglerEnvBindingKind,
};
use indexmap::IndexMap;

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::{
    SymbolKind, SymbolTable, ensure,
    err::{BatchResult, CompilerErrorKind, ErrorSink},
    is_valid_sql_type, kahns,
};

#[derive(Default)]
pub struct ModelAnalysis {
    sink: ErrorSink,
    in_degree: BTreeMap<String, usize>,
    graph: BTreeMap<String, Vec<String>>,
}

impl ModelAnalysis {
    pub fn analyze(
        mut self,
        model_blocks: HashMap<String, &ModelBlock>,
        table: &mut SymbolTable,
    ) -> BatchResult<IndexMap<String, Model>> {
        let mut models: IndexMap<String, Model> = IndexMap::new();

        for model_block in model_blocks.values() {
            let mut model = Model {
                name: model_block.symbol.name.clone(),
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
                self.d1_properties(&mut model, model_block, &model_blocks, table);
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
                        model: model_block.symbol.clone(),
                    });
                }
            }

            model.cruds = model_block.cruds.clone();
            models.insert(model.name.clone(), model);
        }

        // Topologically sort models based on FK relationships
        match kahns(self.graph, self.in_degree, model_blocks.len()) {
            Ok(rank) => {
                models.sort_by_key(|name, _| rank.get(name).copied().unwrap_or(usize::MAX));
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
        model_blocks: &HashMap<String, &ModelBlock>,
        table: &mut SymbolTable,
    ) {
        let model_name = &model_block.symbol.name;
        let model_symbol = &model_block.symbol;

        // All D1 models require a binding
        let Some(d1_binding) = &model_block.d1_binding else {
            self.sink.push(CompilerErrorKind::D1ModelMissingD1Binding {
                model: model_symbol.clone(),
            });
            return;
        };

        let Some(binding_symbol) = table.resolve(
            &d1_binding.env_binding,
            SymbolKind::WranglerEnvBinding {
                kind: WranglerEnvBindingKind::D1,
            },
            None,
        ) else {
            self.sink.push(CompilerErrorKind::D1ModelInvalidD1Binding {
                model: model_symbol.clone(),
                tag: d1_binding.clone(),
            });
            return;
        };

        let binding_name = binding_symbol.name.clone();

        // At least one primary key must be defined
        if model_block.primary_keys.is_empty() {
            self.sink.push(CompilerErrorKind::D1ModelMissingPrimaryKey {
                model: model_symbol.clone(),
            });
            return;
        }

        let mut column_names = HashSet::new();
        let mut primary_column_names = HashSet::new();

        // Column name -> (adj_model_name, adj_column_name, composite_id)
        let mut fk_info: HashMap<String, (String, String, Option<usize>)> = HashMap::new();

        // Column name -> Vec<usize>
        let mut unique_info: HashMap<String, Vec<usize>> = HashMap::new();

        for field in &model_block.fields {
            if !is_valid_sql_type(&field.cidl_type) {
                continue;
            }
            column_names.insert(field.name.clone());

            let is_pk = model_block
                .primary_keys
                .iter()
                .any(|pk| pk.field == field.name);
            if is_pk {
                ensure!(
                    !field.cidl_type.is_nullable(),
                    self.sink,
                    CompilerErrorKind::NullablePrimaryKey {
                        column: field.clone()
                    }
                );
                primary_column_names.insert(field.name.clone());
            }
        }

        self.graph.entry(model_name.clone()).or_default();
        self.in_degree.entry(model_name.clone()).or_insert(0);

        // Foreign keys
        let mut fk_columns_seen = HashSet::<String>::new();
        let mut composite_counter = 0usize;
        for fk in &model_block.foreign_keys {
            self.foreign_key(
                model_block,
                fk,
                &column_names,
                &mut fk_columns_seen,
                table,
                model_blocks,
                &mut fk_info,
                &mut composite_counter,
            );
        }

        // Navigation properties
        let mut navigation_properties = Vec::new();
        let mut nav_fields_seen = HashSet::<String>::new();
        for nav in &model_block.navigation_properties {
            let nav_result = self.nav(model_block, nav, &mut nav_fields_seen, table, model_blocks);

            if let Some(nav) = nav_result {
                navigation_properties.push(nav);
            }
        }

        // Unique constraints
        for (constraint_idx, constraint) in model_block.unique_constraints.iter().enumerate() {
            for column in &constraint.fields {
                if !column_names.contains(column.as_str()) {
                    self.sink.push(
                        CompilerErrorKind::UniqueConstraintReferencesInvalidOrUnknownField {
                            tag: FileSpan::from_simple_span(constraint.span),
                            field: column.clone(),
                        },
                    );
                    continue;
                }
                unique_info
                    .entry(column.clone())
                    .or_default()
                    .push(constraint_idx);
            }
        }

        // Build Column structs
        let mut primary_columns = Vec::new();
        let mut columns = Vec::new();
        for field in &model_block.fields {
            if !column_names.contains(&field.name) {
                continue;
            }

            let foreign_key_reference =
                fk_info
                    .get(&field.name)
                    .map(|(model_name, col_name, _)| ForeignKeyReference {
                        model_name: model_name.clone(),
                        column_name: col_name.clone(),
                    });
            let composite_id = fk_info.get(&field.name).and_then(|(_, _, cid)| *cid);
            let unique_ids_val = unique_info.remove(&field.name).unwrap_or_default();

            let col = Column {
                field: Field {
                    name: field.name.clone(),
                    cidl_type: field.cidl_type.clone(),
                },
                foreign_key_reference,
                unique_ids: unique_ids_val,
                composite_id,
            };

            if primary_column_names.contains(&field.name) {
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
        columns: &HashSet<String>,
        fk_columns_seen: &mut HashSet<String>,
        table: &mut SymbolTable,
        model_blocks: &HashMap<String, &ModelBlock>,
        fk_info: &mut HashMap<String, (String, String, Option<usize>)>,
        composite_counter: &mut usize,
    ) {
        let fk_span = FileSpan::from_simple_span(fk.span);
        let model_name = &model_block.symbol.name;

        // Check that the adjacent model exists
        let Some(adj_model_block) = model_blocks.get(&fk.adj_model) else {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                span: fk_span.clone(),
            });
            return;
        };

        if fk.adj_model == *model_name {
            self.sink.push(CompilerErrorKind::ForeignKeyReferencesSelf {
                model: model_block.symbol.clone(),
                foreign_key: fk_span,
            });
            return;
        }

        // Must belong to the same database
        if model_block.d1_binding.as_ref().map(|t| &t.env_binding)
            != adj_model_block.d1_binding.as_ref().map(|t| &t.env_binding)
        {
            self.sink
                .push(CompilerErrorKind::ForeignKeyReferencesDifferentDatabase {
                    tag: fk_span,
                    binding: adj_model_block
                        .d1_binding
                        .as_ref()
                        .map(|t| t.env_binding.clone())
                        .unwrap_or_default(),
                });
            return;
        }

        let first_ref_field = &fk.references.first().unwrap().0;
        let Some(first_ref_sym) =
            table.resolve(first_ref_field, SymbolKind::ModelField, Some(model_name))
        else {
            self.sink.push(
                CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                    tag: fk_span,
                    column: first_ref_field.clone(),
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

        let adj_model_name = fk.adj_model.clone();

        for (field_name, adj_field_name) in &fk.references {
            let fk_span = fk_span.clone();

            // Validate the field from this model
            let field_cidl_type = {
                // Field should be a column on this model
                if !columns.contains(field_name.as_str()) {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk_span,
                            column: field_name.clone(),
                        },
                    );
                    continue;
                }

                let Some(field_sym) =
                    table.resolve(field_name, SymbolKind::ModelField, Some(model_name))
                else {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk_span,
                            column: field_name.clone(),
                        },
                    );
                    continue;
                };

                // A column cannot be in multiple foreign keys
                if !fk_columns_seen.insert(field_name.clone()) {
                    self.sink
                        .push(CompilerErrorKind::ForeignKeyColumnAlreadyInForeignKey {
                            tag: fk_span.clone(),
                            column: field_sym.clone(),
                        });
                }

                if field_sym.cidl_type.is_nullable() != is_nullable {
                    self.sink
                        .push(CompilerErrorKind::ForeignKeyInconsistentNullability {
                            tag: fk_span.clone(),
                            first_column: first_ref_sym.clone(),
                            second_column: field_sym.clone(),
                        });
                }

                field_sym.cidl_type.clone()
            };

            // Validate the field from the adjacent model
            let adj_field_cidl_type = {
                let Some(adj_field_sym) = table.resolve(
                    adj_field_name,
                    SymbolKind::ModelField,
                    Some(&adj_model_name),
                ) else {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk_span,
                            column: adj_field_name.clone(),
                        },
                    );
                    continue;
                };

                if !is_valid_sql_type(&adj_field_sym.cidl_type) {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk_span.clone(),
                            column: adj_field_name.clone(),
                        },
                    );
                }

                adj_field_sym.cidl_type.clone()
            };

            if field_cidl_type.root_type() != adj_field_cidl_type.root_type() {
                let field_sym = table
                    .resolve(field_name, SymbolKind::ModelField, Some(model_name))
                    .unwrap()
                    .clone();
                let adj_field_sym = table
                    .resolve(
                        adj_field_name,
                        SymbolKind::ModelField,
                        Some(&adj_model_name),
                    )
                    .unwrap()
                    .clone();
                self.sink.push(
                    CompilerErrorKind::ForeignKeyReferencesIncompatibleColumnType {
                        tag: fk_span,
                        column: field_sym,
                        adj_column: adj_field_sym,
                    },
                );
                continue;
            }

            // Store FK info for this column
            fk_info.insert(
                field_name.clone(),
                (adj_model_name.clone(), adj_field_name.clone(), composite_id),
            );

            if !field_cidl_type.is_nullable() {
                // One To One: Person has a Dog ..(sql)=> Person has a fk to Dog
                // Dog must come before Person
                self.graph
                    .entry(adj_model_name.clone())
                    .or_default()
                    .push(model_name.clone());
                *self.in_degree.entry(model_name.clone()).or_insert(0) += 1;
            }
        }
    }

    fn nav(
        &mut self,
        model_block: &ModelBlock,
        nav: &NavigationTag,
        nav_fields_seen: &mut HashSet<String>,
        table: &mut SymbolTable,
        model_blocks: &HashMap<String, &ModelBlock>,
    ) -> Option<NavigationField> {
        let model_name = &model_block.symbol.name;
        let nav_span = FileSpan::from_simple_span(nav.span);

        if !nav_fields_seen.insert(nav.field.clone()) {
            let field_sym = table.resolve(&nav.field, SymbolKind::ModelField, Some(model_name))?;
            self.sink.push(
                CompilerErrorKind::NavigationPropertyFieldAlreadyInNavigationProperty {
                    tag: nav_span,
                    field: field_sym.clone(),
                },
            );
            return None;
        }

        let Some(nav_field_sym) =
            table.resolve(&nav.field, SymbolKind::ModelField, Some(model_name))
        else {
            self.sink
                .push(CompilerErrorKind::UnresolvedSymbol { span: nav_span });
            return None;
        };
        let nav_field_sym = nav_field_sym.clone();

        // Derive the referenced model from the first field's model name
        if nav.fields.is_empty() {
            self.sink
                .push(CompilerErrorKind::UnresolvedSymbol { span: nav_span });
            return None;
        }

        // Validate all referenced fields exist
        let mut referenced_field_names = Vec::new();
        let mut all_valid = true;
        for (ref_model_name, ref_field_name) in &nav.fields {
            let Some(_field_sym) =
                table.resolve(ref_field_name, SymbolKind::ModelField, Some(ref_model_name))
            else {
                self.sink.push(
                    CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                        tag: nav_span.clone(),
                        field: ref_field_name.clone(),
                    },
                );
                all_valid = false;
                continue;
            };
            referenced_field_names.push(ref_field_name.clone());
        }
        if !all_valid {
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

        let adj_model_name = match unwrap_arr_and_null(&nav_field_sym.cidl_type) {
            CidlType::Object { name, .. } => {
                if !model_blocks.contains_key(name.as_str()) {
                    self.sink.push(
                        CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                            tag: nav_span,
                            field: nav.field.clone(),
                        },
                    );
                    return None;
                }
                name.clone()
            }
            _ => {
                self.sink.push(
                    CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                        tag: nav_span,
                        field: nav.field.clone(),
                    },
                );
                return None;
            }
        };

        // Validate the adjacent model is in the same database
        if let Some(adj_block) = model_blocks.get(&adj_model_name) {
            if adj_block.d1_binding.as_ref().map(|t| &t.env_binding)
                != model_block.d1_binding.as_ref().map(|t| &t.env_binding)
            {
                self.sink.push(
                    CompilerErrorKind::NavigationPropertyReferencesDifferentDatabase {
                        tag: nav_span,
                        binding: adj_block
                            .d1_binding
                            .as_ref()
                            .map(|t| t.env_binding.clone())
                            .unwrap_or_default(),
                    },
                );
                return None;
            }
        }

        let nav_field = Field {
            name: nav_field_sym.name.clone(),
            cidl_type: nav_field_sym.cidl_type.clone(),
        };

        let has_arr = matches!(nav_field_sym.cidl_type, CidlType::Array(_));
        let nav_result = match (has_arr, nav.is_many_to_many) {
            (false, false) => {
                // One to One navigation property
                // The current model should have a FK to the adjacent model.
                // The nav's referenced fields can match either side of the FK.
                let has_matching_fk = model_block.foreign_keys.iter().any(|fk| {
                    if fk.adj_model != adj_model_name {
                        return false;
                    }
                    let nav_fields_ref: Vec<&String> = referenced_field_names.iter().collect();

                    // Match against FK source fields (current model columns)
                    let fk_source_fields: Vec<&String> =
                        fk.references.iter().map(|(src, _)| src).collect();
                    if compare_vecs_ignoring_order(&fk_source_fields, &nav_fields_ref) {
                        return true;
                    }

                    // Match against FK adj fields (adjacent model columns)
                    let fk_adj_fields: Vec<&String> =
                        fk.references.iter().map(|(_, adj)| adj).collect();
                    compare_vecs_ignoring_order(&fk_adj_fields, &nav_fields_ref)
                });

                ensure!(
                    has_matching_fk,
                    self.sink,
                    CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                        tag: nav_span,
                        field: nav.field.clone(),
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
                let adj_block = model_blocks.get(&adj_model_name);
                let has_matching_fk = adj_block.map_or(false, |ab| {
                    ab.foreign_keys.iter().any(|fk| {
                        let fk_fields: Vec<&String> =
                            fk.references.iter().map(|(field, _)| field).collect();
                        let nav_fields_ref: Vec<&String> = referenced_field_names.iter().collect();

                        compare_vecs_ignoring_order(&fk_fields, &nav_fields_ref)
                            && fk.adj_model == *model_name
                    })
                });

                ensure!(
                    has_matching_fk,
                    self.sink,
                    CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                        tag: nav_span,
                        field: nav.field.clone(),
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
                let adj_block = model_blocks.get(&adj_model_name);
                let matching_nav_count = adj_block.map_or(0, |ab| {
                    ab.navigation_properties
                        .iter()
                        .filter(|adj_nav| {
                            adj_nav.is_many_to_many
                                && adj_nav
                                    .fields
                                    .first()
                                    .map(|(m, _)| m == model_name)
                                    .unwrap_or(false)
                        })
                        .count()
                });

                if matching_nav_count == 0 {
                    self.sink
                        .push(CompilerErrorKind::NavigationPropertyMissingReciprocalM2M {
                            tag: nav_span,
                        });
                    return None;
                }

                ensure!(
                    matching_nav_count == 1,
                    self.sink,
                    CompilerErrorKind::NavigationPropertyAmbiguousM2M { tag: nav_span }
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
                        tag: nav_span,
                        field: nav.field.clone(),
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
        let model_name = &model_block.symbol.name;

        // Validates that a KV/R2 tag's env binding exists and is of the correct WranglerEnvBindingKind
        let validate_binding = |sink: &mut ErrorSink,
                                tag: &KvR2Tag,
                                expected: WranglerEnvBindingKind|
         -> Option<String> {
            let tag_span = FileSpan::from_simple_span(tag.span);

            let Some(binding_sym) = table.resolve(
                &tag.env_binding,
                SymbolKind::WranglerEnvBinding {
                    kind: expected.clone(),
                },
                None,
            ) else {
                let err = match expected {
                    WranglerEnvBindingKind::Kv => CompilerErrorKind::KvInvalidBinding {
                        tag: tag_span,
                        binding: tag.env_binding.clone(),
                    },
                    WranglerEnvBindingKind::R2 => CompilerErrorKind::R2InvalidBinding {
                        tag: tag_span,
                        binding: tag.env_binding.clone(),
                    },
                    _ => CompilerErrorKind::UnresolvedSymbol { span: tag_span },
                };
                sink.push(err);
                return None;
            };

            Some(binding_sym.name.clone())
        };

        // Extracts variables from a formatted string, then validates that they
        // correspond to fields on the models that are of valid SQLite types
        let validate_key_format =
            |sink: &mut ErrorSink, tag_span: FileSpan, format: &str| -> bool {
                let vars = match extract_braced(format) {
                    Ok(vars) => vars,
                    Err(reason) => {
                        sink.push(CompilerErrorKind::KvR2InvalidKeyFormat {
                            tag: tag_span,
                            reason,
                        });
                        return false;
                    }
                };

                for var in vars {
                    // Look through fields for a matching name
                    let matching_field = model_block
                        .fields
                        .iter()
                        .find(|f| f.name == var && is_valid_sql_type(&f.cidl_type));

                    if matching_field.is_none() {
                        sink.push(CompilerErrorKind::KvR2UnknownKeyVariable {
                            tag: tag_span.clone(),
                            variable: var,
                        });
                        return false;
                    }
                }

                true
            };

        for kv in &model_block.kvs {
            let kv_span = FileSpan::from_simple_span(kv.span);
            let binding_name = validate_binding(&mut self.sink, kv, WranglerEnvBindingKind::Kv);

            if !validate_key_format(&mut self.sink, kv_span.clone(), &kv.format) {
                continue;
            }

            let Some(field_sym) =
                table.resolve(&kv.field, SymbolKind::ModelField, Some(model_name))
            else {
                self.sink
                    .push(CompilerErrorKind::UnresolvedSymbol { span: kv_span });
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
            let r2_span = FileSpan::from_simple_span(r2.span);
            let binding_name = validate_binding(&mut self.sink, r2, WranglerEnvBindingKind::R2);

            let Some(field_sym) =
                table.resolve(&r2.field, SymbolKind::ModelField, Some(model_name))
            else {
                self.sink
                    .push(CompilerErrorKind::UnresolvedSymbol { span: r2_span });
                continue;
            };

            if !validate_key_format(&mut self.sink, r2_span.clone(), &r2.format) {
                continue;
            }

            if field_sym.cidl_type != CidlType::R2Object {
                self.sink.push(CompilerErrorKind::KvR2InvalidField {
                    tag: r2_span,
                    field: r2.field.clone(),
                });
                continue;
            }

            model.r2_fields.push(KvR2Field {
                name: field_sym.name.clone(),
                cidl_type: field_sym.cidl_type.clone(),
                format: r2.format.clone(),
                binding: binding_name.unwrap_or_default(),
                list_prefix: false,
            });
        }

        for key_field in &model_block.key_fields {
            let kf_span = FileSpan::from_simple_span(key_field.span);
            let Some(field_sym) =
                table.resolve(&key_field.field, SymbolKind::ModelField, Some(model_name))
            else {
                self.sink
                    .push(CompilerErrorKind::UnresolvedSymbol { span: kf_span });
                continue;
            };

            if field_sym.cidl_type != CidlType::String {
                self.sink.push(CompilerErrorKind::KvR2InvalidKeyParam {
                    tag: kf_span,
                    field: field_sym.clone(),
                });
                continue;
            }

            model.key_fields.push(field_sym.name.clone());
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
