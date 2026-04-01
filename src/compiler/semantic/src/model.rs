use crate::{
    SymbolKind, SymbolTable, ensure,
    err::{BatchResult, ErrorSink, SemanticError},
    is_valid_sql_type, kahns, resolve_cidl_type,
};
use ast::{
    CidlType, Column, CrudKind, Field, ForeignKeyReference, KvR2Field, Model, NavigationField,
    NavigationFieldKind,
};
use frontend::{ForeignKeyTag, KvR2Tag, ModelBlock, NavigationTag, Span, WranglerEnvBindingKind};
use indexmap::IndexMap;
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Default)]
pub struct ModelAnalysis<'src, 'p> {
    sink: ErrorSink<'src, 'p>,
    in_degree: BTreeMap<&'src str, usize>,
    graph: BTreeMap<&'src str, Vec<&'src str>>,
}

impl<'src, 'p> ModelAnalysis<'src, 'p> {
    pub fn analyze(
        mut self,
        model_blocks: HashMap<&'src str, &'p ModelBlock<'src>>,
        table: &mut SymbolTable<'src, 'p>,
    ) -> BatchResult<'src, 'p, IndexMap<&'src str, Model<'src>>> {
        let mut models: IndexMap<&'src str, Model<'src>> = IndexMap::new();

        for &model_block in model_blocks.values() {
            let mut model = Model {
                name: model_block.symbol.name,
                ..Default::default()
            };

            // If any D1 properties occur, treat the model as a D1 model
            if model_block.d1_binding.is_some()
                || !model_block.foreign_keys.is_empty()
                || !model_block.navigation_properties.is_empty()
                || !model_block.primary_keys.is_empty()
            {
                self.d1_properties(&mut model, model_block, &model_blocks, table);
            }

            // If any KV/R2 properties occur, validate them and set the model's KV/R2 fields
            if !model_block.kvs.is_empty()
                || !model_block.r2s.is_empty()
                || !model_block.key_fields.is_empty()
            {
                self.kv_r2_properties(&mut model, model_block, table);
            }

            // Validate CRUD
            for crud in &model_block.cruds {
                ensure!(
                    !matches!(crud, CrudKind::List) || model.d1_binding.is_some(),
                    self.sink,
                    SemanticError::UnsupportedCrudOperation {
                        model: &model_block.symbol
                    }
                );
            }

            model.cruds = model_block.cruds.clone();
            models.insert(model.name, model);
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
        model: &mut Model<'src>,
        model_block: &'p ModelBlock<'src>,
        model_blocks: &HashMap<&'src str, &'p ModelBlock<'src>>,
        table: &mut SymbolTable<'src, 'p>,
    ) {
        let model_name = model_block.symbol.name;
        let model_symbol = &model_block.symbol;

        // All D1 models require a binding
        let binding_symbol = {
            let Some(d1_binding) = &model_block.d1_binding else {
                self.sink.push(SemanticError::D1ModelMissingD1Binding {
                    model: model_symbol,
                });
                return;
            };

            let Some(binding_symbol) = table.resolve(
                d1_binding.env_binding,
                SymbolKind::WranglerEnvBinding {
                    kind: WranglerEnvBindingKind::D1,
                },
                None,
            ) else {
                self.sink.push(SemanticError::D1ModelInvalidD1Binding {
                    model: model_symbol,
                    tag: d1_binding,
                });
                return;
            };

            binding_symbol.clone()
        };

        // At least one primary key must be defined
        ensure!(
            !model_block.primary_keys.is_empty(),
            self.sink,
            SemanticError::D1ModelMissingPrimaryKey {
                model: model_symbol,
            }
        );

        let mut column_names: HashSet<&str> = HashSet::new();
        let mut primary_column_names: HashSet<&str> = HashSet::new();

        // Column name -> (adj_model_name, adj_column_name, composite_id)
        let mut fk_info: HashMap<&str, (&str, &str, Option<usize>)> = HashMap::new();

        // Column name -> Vec<usize>
        let mut unique_info: HashMap<&str, Vec<usize>> = HashMap::new();

        // Validate columns
        for field in &model_block.fields {
            let resolved_type = match resolve_cidl_type(field, &field.cidl_type, table) {
                Ok(t) => t,
                Err(err) => {
                    self.sink.push(err);
                    continue;
                }
            };

            let is_key_field = model_block
                .key_fields
                .iter()
                .any(|kf| kf.field == field.name);
            if !is_valid_sql_type(&resolved_type) || is_key_field {
                continue;
            }
            column_names.insert(field.name);

            let is_pk = model_block
                .primary_keys
                .iter()
                .any(|pk| pk.field == field.name);
            if is_pk {
                if field.cidl_type.is_nullable() {
                    self.sink
                        .push(SemanticError::NullablePrimaryKey { column: field });
                    continue;
                }

                primary_column_names.insert(field.name);
            }
        }

        self.graph.entry(model_name).or_default();
        self.in_degree.entry(model_name).or_insert(0);

        // Foreign keys
        let mut fk_columns_seen = HashSet::<&str>::new();
        let mut composite_counter = 0usize;
        for fk in &model_block.foreign_keys {
            self.foreign_key(
                model_block,
                &column_names,
                fk,
                &mut fk_columns_seen,
                &mut fk_info,
                &mut composite_counter,
                table,
                model_blocks,
            );
        }

        // Navigation properties
        let mut navigation_properties = Vec::new();
        let mut nav_fields_seen = HashSet::<&'src str>::new();
        for nav in &model_block.navigation_properties {
            if let Some(nav) = self.nav(model_block, nav, &mut nav_fields_seen, table, model_blocks)
            {
                navigation_properties.push(nav);
            }
        }

        // Unique constraints
        for (constraint_idx, constraint) in model_block.unique_constraints.iter().enumerate() {
            for &column in &constraint.fields {
                if !column_names.contains(column) {
                    self.sink.push(
                        SemanticError::UniqueConstraintReferencesInvalidOrUnknownField {
                            span: constraint.span,
                            field: column,
                        },
                    );
                    continue;
                }

                unique_info.entry(column).or_default().push(constraint_idx);
            }
        }

        // Build Column structs
        let mut primary_columns = Vec::new();
        let mut columns = Vec::new();
        for field in &model_block.fields {
            if !column_names.contains(field.name) {
                continue;
            }

            let foreign_key_reference =
                fk_info
                    .get(field.name)
                    .map(|(model_name, column_name, _)| ForeignKeyReference {
                        model_name,
                        column_name,
                    });
            let composite_id = fk_info.get(field.name).and_then(|(_, _, cid)| *cid);
            let unique_ids_val = unique_info.remove(field.name).unwrap_or_default();

            let col = Column {
                hash: 0,
                field: Field {
                    name: field.name.into(),
                    cidl_type: field.cidl_type.clone(),
                },
                foreign_key_reference,
                unique_ids: unique_ids_val,
                composite_id,
            };

            if primary_column_names.contains(field.name) {
                primary_columns.push(col);
            } else {
                columns.push(col);
            }
        }

        // Set D1 model properties
        model.d1_binding = Some(binding_symbol.name);
        model.columns = columns;
        model.primary_columns = primary_columns;
        model.navigation_fields = navigation_properties;
    }

    /// Validates a foreign key and populates fk_info map
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn foreign_key(
        &mut self,
        model_block: &'p ModelBlock<'src>,
        columns: &HashSet<&'src str>,
        fk: &'p ForeignKeyTag<'src>,
        fk_columns_seen: &mut HashSet<&'src str>,
        fk_info: &mut HashMap<&'src str, (&'src str, &'src str, Option<usize>)>,
        composite_counter: &mut usize,
        table: &mut SymbolTable<'src, 'p>,
        model_blocks: &HashMap<&'src str, &'p ModelBlock<'src>>,
    ) {
        let model_name = model_block.symbol.name;

        // Check that the adjacent model exists
        let Some(adj_model_block) = model_blocks.get(fk.adj_model) else {
            self.sink
                .push(SemanticError::UnresolvedSymbol { span: fk.span });
            return;
        };

        if fk.adj_model == model_name {
            self.sink.push(SemanticError::ForeignKeyReferencesSelf {
                model: &model_block.symbol,
                foreign_key: fk.span,
            });
            return;
        }

        // Must belong to the same database
        if model_block.d1_binding.as_ref().map(|t| &t.env_binding)
            != adj_model_block.d1_binding.as_ref().map(|t| &t.env_binding)
        {
            self.sink
                .push(SemanticError::ForeignKeyReferencesDifferentDatabase {
                    span: fk.span,
                    binding: adj_model_block
                        .d1_binding
                        .as_ref()
                        .map(|t| t.env_binding)
                        .unwrap_or_default(),
                });
            return;
        }

        let first_ref_field = &fk.references.first().unwrap().0;
        let Some(first_ref_sym) =
            table.resolve(first_ref_field, SymbolKind::ModelField, Some(model_name))
        else {
            self.sink
                .push(SemanticError::ForeignKeyReferencesInvalidOrUnknownColumn {
                    span: fk.span,
                    column: first_ref_field,
                });
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

        let adj_model_name = fk.adj_model;

        for (field_name, adj_field_name) in &fk.references {
            // Validate the field from this model
            let field_cidl_type = {
                // Field should be a column on this model
                if !columns.contains(field_name) {
                    self.sink
                        .push(SemanticError::ForeignKeyReferencesInvalidOrUnknownColumn {
                            span: fk.span,
                            column: field_name,
                        });
                    continue;
                }

                let Some(field_sym) =
                    table.resolve(field_name, SymbolKind::ModelField, Some(model_name))
                else {
                    self.sink
                        .push(SemanticError::ForeignKeyReferencesInvalidOrUnknownColumn {
                            span: fk.span,
                            column: field_name,
                        });
                    continue;
                };

                // A column cannot be in multiple foreign keys
                if !fk_columns_seen.insert(field_name) {
                    self.sink
                        .push(SemanticError::ForeignKeyColumnAlreadyInForeignKey {
                            span: fk.span,
                            column: &field_sym,
                        });
                }

                if field_sym.cidl_type.is_nullable() != is_nullable {
                    self.sink
                        .push(SemanticError::ForeignKeyInconsistentNullability {
                            span: fk.span,
                            first_column: &first_ref_sym,
                            second_column: &field_sym,
                        });
                }

                field_sym.cidl_type.clone()
            };

            // Validate the field from the adjacent model
            let adj_field_cidl_type = {
                let Some(adj_field_sym) =
                    table.resolve(adj_field_name, SymbolKind::ModelField, Some(adj_model_name))
                else {
                    self.sink
                        .push(SemanticError::ForeignKeyReferencesInvalidOrUnknownColumn {
                            span: fk.span,
                            column: adj_field_name,
                        });
                    continue;
                };

                if !is_valid_sql_type(&adj_field_sym.cidl_type) {
                    self.sink
                        .push(SemanticError::ForeignKeyReferencesInvalidOrUnknownColumn {
                            span: fk.span,
                            column: adj_field_name,
                        });
                }

                adj_field_sym.cidl_type.clone()
            };

            if field_cidl_type.root_type() != adj_field_cidl_type.root_type() {
                let column = table
                    .resolve(field_name, SymbolKind::ModelField, Some(model_name))
                    .unwrap();
                let adj_column = table
                    .resolve(adj_field_name, SymbolKind::ModelField, Some(adj_model_name))
                    .unwrap();

                self.sink
                    .push(SemanticError::ForeignKeyReferencesIncompatibleColumnType {
                        span: fk.span,
                        column: &column,
                        adj_column: &adj_column,
                    });
                continue;
            }

            // Store FK info for this column
            fk_info.insert(*field_name, (adj_model_name, *adj_field_name, composite_id));

            if !field_cidl_type.is_nullable() {
                // One To One: Person has a Dog ..(sql)=> Person has a fk to Dog
                // Dog must come before Person
                self.graph
                    .entry(adj_model_name)
                    .or_default()
                    .push(model_name);
                *self.in_degree.entry(model_name).or_insert(0) += 1;
            }
        }
    }

    fn nav(
        &mut self,
        model_block: &'p ModelBlock<'src>,
        nav: &'p NavigationTag<'src>,
        nav_fields_seen: &mut HashSet<&'src str>,
        table: &mut SymbolTable<'src, 'p>,
        model_blocks: &HashMap<&'src str, &'p ModelBlock<'src>>,
    ) -> Option<NavigationField<'src>> {
        let model_name = model_block.symbol.name;

        let Some(nav_field_sym) =
            table.resolve(nav.field, SymbolKind::ModelField, Some(model_name))
        else {
            self.sink
                .push(SemanticError::UnresolvedSymbol { span: nav.span });
            return None;
        };

        // A nav field cannot be in multiple navigation properties
        if !nav_fields_seen.insert(nav.field) {
            self.sink.push(
                SemanticError::NavigationPropertyFieldAlreadyInNavigationProperty {
                    span: nav.span,
                    field: nav_field_sym,
                },
            );
            return None;
        }

        // Validate all referenced fields exist
        let mut referenced_field_names = Vec::new();
        {
            let mut all_valid = true;
            for (ref_model_name, ref_field_name) in &nav.fields {
                let Some(_field_sym) =
                    table.resolve(ref_field_name, SymbolKind::ModelField, Some(ref_model_name))
                else {
                    self.sink.push(
                        SemanticError::NavigationPropertyReferencesInvalidOrUnknownField {
                            span: nav.span,
                            field: ref_field_name,
                        },
                    );
                    all_valid = false;
                    continue;
                };
                referenced_field_names.push(*ref_field_name);
            }
            if !all_valid {
                return None;
            }
        }

        let resolved_nav_field_type =
            match resolve_cidl_type(nav_field_sym, &nav_field_sym.cidl_type, table) {
                Ok(t) => t,
                Err(err) => {
                    self.sink.push(err);
                    return None;
                }
            };

        // A nav field must be of cidl type Object, that Object must be the adjacent model
        // OR an array/nullable wrapper around the adjacent model.
        let base_type = match resolved_nav_field_type.clone() {
            CidlType::Array(inner) => *inner,
            CidlType::Nullable(inner) => *inner,
            other => other,
        };

        let adj_model_name = match base_type {
            CidlType::Object { name, .. } => {
                if !model_blocks.contains_key(name) {
                    self.sink.push(
                        SemanticError::NavigationPropertyReferencesInvalidOrUnknownField {
                            span: nav.span,
                            field: nav.field,
                        },
                    );
                    return None;
                }
                name
            }
            _ => {
                self.sink.push(
                    SemanticError::NavigationPropertyReferencesInvalidOrUnknownField {
                        span: nav.span,
                        field: nav.field,
                    },
                );
                return None;
            }
        };

        // Validate the adjacent model is in the same database
        if let Some(adj_block) = model_blocks.get(&adj_model_name)
            && adj_block.d1_binding.as_ref().map(|t| &t.env_binding)
                != model_block.d1_binding.as_ref().map(|t| &t.env_binding)
        {
            self.sink.push(
                SemanticError::NavigationPropertyReferencesDifferentDatabase {
                    span: nav.span,
                    binding: adj_block
                        .d1_binding
                        .as_ref()
                        .map(|t| t.env_binding)
                        .unwrap_or_default(),
                },
            );
            return None;
        }

        let nav_field = Field {
            name: nav_field_sym.name.into(),
            cidl_type: resolved_nav_field_type.clone(),
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
                    let nav_fields_ref: Vec<&&'src str> = referenced_field_names.iter().collect();

                    // Match against FK source fields (current model columns)
                    let fk_source_fields: Vec<&&'src str> =
                        fk.references.iter().map(|(src, _)| src).collect();
                    if compare_vecs_ignoring_order(&fk_source_fields, &nav_fields_ref) {
                        return true;
                    }

                    // Match against FK adj fields (adjacent model columns)
                    let fk_adj_fields: Vec<&&'src str> =
                        fk.references.iter().map(|(_, adj)| adj).collect();
                    compare_vecs_ignoring_order(&fk_adj_fields, &nav_fields_ref)
                });

                ensure!(
                    has_matching_fk,
                    self.sink,
                    SemanticError::NavigationPropertyReferencesInvalidOrUnknownField {
                        span: nav.span,
                        field: nav.field,
                    }
                );

                NavigationField {
                    hash: 0,
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
                let has_matching_fk = adj_block.is_some_and(|ab| {
                    ab.foreign_keys.iter().any(|fk| {
                        let fk_fields: Vec<&&'src str> =
                            fk.references.iter().map(|(field, _)| field).collect();
                        let nav_fields_ref: Vec<&&'src str> =
                            referenced_field_names.iter().collect();

                        compare_vecs_ignoring_order(&fk_fields, &nav_fields_ref)
                            && fk.adj_model == model_name
                    })
                });

                ensure!(
                    has_matching_fk,
                    self.sink,
                    SemanticError::NavigationPropertyReferencesInvalidOrUnknownField {
                        span: nav.span,
                        field: nav.field,
                    }
                );

                NavigationField {
                    hash: 0,
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
                                    .map(|(m, _)| *m == model_name)
                                    .unwrap_or(false)
                        })
                        .count()
                });

                if matching_nav_count == 0 {
                    self.sink
                        .push(SemanticError::NavigationPropertyMissingReciprocalM2M {
                            span: nav.span,
                        });
                    return None;
                }

                ensure!(
                    matching_nav_count == 1,
                    self.sink,
                    SemanticError::NavigationPropertyAmbiguousM2M { span: nav.span }
                );

                NavigationField {
                    hash: 0,
                    field: nav_field,
                    model_reference: adj_model_name,
                    kind: NavigationFieldKind::ManyToMany,
                }
            }
            _ => {
                self.sink.push(
                    SemanticError::NavigationPropertyReferencesInvalidOrUnknownField {
                        span: nav.span,
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
        model: &mut Model<'src>,
        model_block: &'p ModelBlock<'src>,
        table: &SymbolTable<'src, 'p>,
    ) {
        let model_name = model_block.symbol.name;

        // Validates that a KV/R2 tag's env binding exists and is of the correct WranglerEnvBindingKind
        let validate_binding = |sink: &mut ErrorSink<'src, 'p>,
                                tag: &KvR2Tag<'src>,
                                expected: WranglerEnvBindingKind|
         -> Option<&'src str> {
            if let Some(binding_sym) = table.resolve(
                tag.env_binding,
                SymbolKind::WranglerEnvBinding {
                    kind: expected.clone(),
                },
                None,
            ) {
                return Some(binding_sym.name);
            }

            let err = match expected {
                WranglerEnvBindingKind::Kv => SemanticError::KvInvalidBinding {
                    span: tag.span,
                    binding: tag.env_binding,
                },
                WranglerEnvBindingKind::R2 => SemanticError::R2InvalidBinding {
                    span: tag.span,
                    binding: tag.env_binding,
                },
                _ => SemanticError::UnresolvedSymbol { span: tag.span },
            };
            sink.push(err);
            None
        };

        // Extracts variables from a formatted string, then validates that they
        // correspond to fields on the models that are of valid SQLite types
        let validate_key_format =
            |sink: &mut ErrorSink<'src, 'p>, span: Span, format: &'src str| -> bool {
                let vars = match extract_braced(format) {
                    Ok(vars) => vars,
                    Err(reason) => {
                        sink.push(SemanticError::KvR2InvalidKeyFormat { span, reason });
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
                        sink.push(SemanticError::KvR2UnknownKeyVariable {
                            span,
                            variable: var,
                        });
                        return false;
                    }
                }

                true
            };

        for kv in &model_block.kvs {
            let binding_name = validate_binding(&mut self.sink, kv, WranglerEnvBindingKind::Kv);

            if !validate_key_format(&mut self.sink, kv.span, kv.format) {
                continue;
            }

            let Some(field_sym) = table.resolve(kv.field, SymbolKind::ModelField, Some(model_name))
            else {
                self.sink
                    .push(SemanticError::UnresolvedSymbol { span: kv.span });
                continue;
            };

            // Always wrap in KvObject
            let resolved_type = match resolve_cidl_type(field_sym, &field_sym.cidl_type, table) {
                Ok(t) => t,
                Err(err) => {
                    self.sink.push(err);
                    continue;
                }
            };
            let cidl_type = match &resolved_type {
                CidlType::Paginated(inner) => {
                    CidlType::paginated(CidlType::KvObject(inner.clone()))
                }
                _ => CidlType::KvObject(Box::new(resolved_type.clone())),
            };

            model.kv_fields.push(KvR2Field {
                field: Field {
                    name: field_sym.name.into(),
                    cidl_type,
                },
                format: kv.format,
                binding: binding_name.unwrap_or_default(),
                list_prefix: false,
            });
        }

        for r2 in &model_block.r2s {
            let binding_name = validate_binding(&mut self.sink, r2, WranglerEnvBindingKind::R2);

            let Some(field_sym) = table.resolve(r2.field, SymbolKind::ModelField, Some(model_name))
            else {
                self.sink
                    .push(SemanticError::UnresolvedSymbol { span: r2.span });
                continue;
            };

            if !validate_key_format(&mut self.sink, r2.span, r2.format) {
                continue;
            }

            if field_sym.cidl_type != CidlType::R2Object
                && field_sym.cidl_type != CidlType::paginated(CidlType::R2Object)
            {
                self.sink.push(SemanticError::KvR2InvalidField {
                    span: r2.span,
                    field: r2.field,
                });
                continue;
            }

            model.r2_fields.push(KvR2Field {
                field: Field {
                    name: field_sym.name.into(),
                    cidl_type: field_sym.cidl_type.clone(),
                },
                format: r2.format,
                binding: binding_name.unwrap_or_default(),
                list_prefix: false,
            });
        }

        for kf in &model_block.key_fields {
            let Some(field_sym) = table.resolve(kf.field, SymbolKind::ModelField, Some(model_name))
            else {
                self.sink
                    .push(SemanticError::UnresolvedSymbol { span: kf.span });
                continue;
            };

            if field_sym.cidl_type != CidlType::String {
                self.sink.push(SemanticError::KvR2InvalidKeyParam {
                    span: kf.span,
                    field: field_sym,
                });
                continue;
            }

            model.key_fields.push(field_sym.name);
        }
    }
}

fn compare_vecs_ignoring_order<T: Ord>(a: &[T], b: &[T]) -> bool {
    if a.len() != b.len() {
        return false;
    }

    let mut a_sorted: Vec<&T> = a.iter().collect();
    a_sorted.sort();

    let mut b_sorted: Vec<&T> = b.iter().collect();
    b_sorted.sort();

    a_sorted == b_sorted
}

/// Extracts braced variables from a format string.
/// e.g, "users/{userId}/posts/{postId}" => ["userId", "postId"].
///
/// Returns an error string if the format string is invalid.
fn extract_braced(s: &str) -> Result<Vec<&str>, String> {
    let mut out = Vec::new();
    let mut current = None;
    let chars = s.char_indices().peekable();
    for (i, c) in chars {
        match (current.is_some(), c) {
            (false, '{') => {
                current = Some(i + 1);
            }
            (true, '{') => {
                return Err("nested brace in key".to_string());
            }
            (true, '}') => {
                let start_idx = current.take().unwrap();
                out.push(&s[start_idx..i]);
            }
            (true, _) => {}
            _ => {}
        }
    }
    if current.is_some() {
        return Err("unclosed brace in key".to_string());
    }
    Ok(out)
}
