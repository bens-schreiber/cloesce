use crate::{
    SymbolKind, SymbolTable, ensure,
    err::{BatchResult, ErrorSink, SemanticError},
    is_valid_sql_type, kahns, resolve_cidl_type,
};
use ast::{
    CidlType, Column, CrudKind, Field, ForeignKeyReference, KvR2Field, Model, NavigationField,
    NavigationFieldKind,
};
use frontend::{ModelBlock, NavigationBlock, Span, Symbol, WranglerEnvBindingKind};
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
        model_blocks: BTreeMap<&'src str, &'p ModelBlock<'src>>,
        table: &mut SymbolTable<'src, 'p>,
    ) -> BatchResult<'src, 'p, IndexMap<&'src str, Model<'src>>> {
        let mut models: IndexMap<&'src str, Model<'src>> = IndexMap::new();

        for &model_block in model_blocks.values() {
            let mut model = Model {
                name: model_block.symbol.name,
                ..Default::default()
            };

            // If any D1 properties occur, treat the model as a D1 model
            let has_tag = model_block
                .use_tag
                .as_ref()
                .is_some_and(|tag| !tag.env_bindings.is_empty());
            if has_tag
                || !model_block.foreign_blocks.is_empty()
                || !model_block.navigation_blocks.is_empty()
                || !model_block.primary_fields.is_empty()
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
            if let Some(tag) = &model_block.use_tag {
                let mut seen_cruds = HashSet::new();
                for crud in &tag.cruds {
                    ensure!(
                        !matches!(crud, CrudKind::List) || model.d1_binding.is_some(),
                        self.sink,
                        SemanticError::UnsupportedCrudOperation {
                            model: &model_block.symbol
                        }
                    );

                    seen_cruds.insert(crud);
                }

                let mut cruds: Vec<CrudKind> = seen_cruds.into_iter().cloned().collect();
                // Sort for deterministic output: Get, List, Save
                cruds.sort_by_key(|c| match c {
                    CrudKind::Get => 0,
                    CrudKind::List => 1,
                    CrudKind::Save => 2,
                });
                model.cruds = cruds;
            }

            models.insert(model.name, model);
        }

        // Topologically sort models based on FK relationships
        match kahns(self.graph, self.in_degree, model_blocks.len()) {
            Ok(rank) => {
                models.sort_by(|a_name, _, b_name, _| {
                    let a_rank = rank.get(a_name).copied().unwrap_or(usize::MAX);
                    let b_rank: usize = rank.get(b_name).copied().unwrap_or(usize::MAX);
                    a_rank.cmp(&b_rank).then_with(|| a_name.cmp(b_name))
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
        model: &mut Model<'src>,
        model_block: &'p ModelBlock<'src>,
        model_blocks: &BTreeMap<&'src str, &'p ModelBlock<'src>>,
        table: &mut SymbolTable<'src, 'p>,
    ) {
        let model_name = model_block.symbol.name;
        let model_symbol = &model_block.symbol;

        self.graph.entry(model_name).or_default();
        self.in_degree.entry(model_name).or_insert(0);

        // All D1 models require a binding
        let binding_symbol = {
            let use_tag_bindings = model_block
                .use_tag
                .as_ref()
                .map(|tag| tag.env_bindings.clone());

            let Some(bindings) = use_tag_bindings else {
                self.sink.push(SemanticError::D1ModelMissingD1Binding {
                    model: model_symbol,
                });
                return;
            };

            if bindings.is_empty() {
                self.sink.push(SemanticError::D1ModelMissingD1Binding {
                    model: model_symbol,
                });
                return;
            }

            if bindings.len() > 1 {
                self.sink.push(SemanticError::D1ModelMultipleD1Bindings {
                    model: model_symbol,
                    bindings,
                });
                return;
            }
            let tag = bindings[0];

            let Some(binding_symbol) = table.resolve(
                tag,
                SymbolKind::WranglerEnvBinding {
                    kind: WranglerEnvBindingKind::D1,
                },
                None,
            ) else {
                self.sink.push(SemanticError::D1ModelInvalidD1Binding {
                    model: model_symbol,
                    binding: tag,
                });
                return;
            };

            binding_symbol
        };

        // At least one primary key must be defined
        ensure!(
            !model_block.primary_fields.is_empty(),
            self.sink,
            SemanticError::D1ModelMissingPrimaryKey {
                model: model_symbol,
            }
        );

        // Unique constraints
        let mut unique_info: HashMap<&str, Vec<usize>> = HashMap::new();
        for (constraint_idx, constraint) in model_block.unique_constraints.iter().enumerate() {
            for &column in &constraint.fields {
                unique_info.entry(column).or_default().push(constraint_idx);
            }
        }

        // Build foreign keys
        let mut columns = Vec::new();
        let mut primary_columns = Vec::new();
        let mut composite_counter = 0usize;
        for fk in &model_block.foreign_blocks {
            let adj_model_name = fk.adj.first().map(|(m, _)| *m).unwrap_or("");

            // Check that the adjacent model exists
            let Some(adj_model_block) = model_blocks.get(adj_model_name) else {
                self.sink.push(SemanticError::UnresolvedSymbol {
                    span: fk.span,
                    name: adj_model_name,
                });
                continue;
            };

            if adj_model_name == model_name {
                self.sink.push(SemanticError::ForeignKeyReferencesSelf {
                    model: &model_block.symbol,
                    foreign_key: fk.span,
                });
                continue;
            }

            // Must belong to the same database
            let adj_binding = adj_model_block
                .use_tag
                .as_ref()
                .and_then(|tag| tag.env_bindings.first().copied());
            if Some(binding_symbol.name) != adj_binding {
                self.sink
                    .push(SemanticError::ForeignKeyReferencesDifferentDatabase {
                        span: fk.span,
                        binding: adj_binding.unwrap_or("no binding"),
                    });
                continue;
            }

            // All adj entries must reference the same model
            if let Some((inconsistent_model, _)) = fk.adj.iter().find(|(m, _)| *m != adj_model_name)
            {
                self.sink.push(SemanticError::InconsistentModelAdjacency {
                    span: fk.span,
                    first_model: adj_model_name,
                    second_model: inconsistent_model,
                });
                continue;
            }

            // Number of adj references must match number of local fields
            if fk.adj.len() != fk.fields.len() {
                self.sink
                    .push(SemanticError::ForeignKeyInconsistentFieldAdj {
                        span: fk.span,
                        adj_count: fk.adj.len(),
                        field_count: fk.fields.len(),
                    });
                continue;
            }

            let composite_id = if fk.adj.len() > 1 {
                let id = composite_counter;
                composite_counter += 1;
                Some(id)
            } else {
                None
            };

            for (field, (_, adj_field_name)) in fk.fields.iter().zip(&fk.adj) {
                let is_pk = model_block.primary_fields.contains(&field.name);
                if is_pk && fk.optional {
                    self.sink
                        .push(SemanticError::NullablePrimaryKey { column: field });
                    continue;
                }

                // Validate the field from the adjacent model
                let Some(adj_field_sym) =
                    table.resolve(adj_field_name, SymbolKind::ModelField, Some(adj_model_name))
                else {
                    self.sink.push(SemanticError::UnresolvedSymbol {
                        span: fk.span,
                        name: adj_field_name,
                    });
                    continue;
                };

                if !is_valid_sql_type(&adj_field_sym.cidl_type) {
                    self.sink.push(SemanticError::ForeignKeyInvalidColumnType {
                        span: fk.span,
                        field: adj_field_sym,
                    });
                }

                if !fk.optional {
                    // One To One: Person has a Dog ..(sql)=> Person has a fk to Dog
                    // Dog must come before Person
                    self.graph
                        .entry(adj_model_name)
                        .or_default()
                        .push(model_name);
                    *self.in_degree.entry(model_name).or_insert(0) += 1;
                }

                let unique_ids = unique_info.remove(field.name).unwrap_or_default();

                let col = Column {
                    hash: 0,
                    field: Field {
                        name: field.name.into(),
                        cidl_type: if fk.optional {
                            CidlType::nullable(adj_field_sym.cidl_type.clone())
                        } else {
                            adj_field_sym.cidl_type.clone()
                        },
                    },
                    foreign_key_reference: Some(ForeignKeyReference {
                        model_name: adj_model_name,
                        column_name: adj_field_name,
                    }),
                    unique_ids,
                    composite_id,
                };

                if is_pk {
                    primary_columns.push(col);
                } else {
                    columns.push(col);
                }
            }
        }

        // Build Navigation properties
        let mut navigation_properties = Vec::new();
        for nav in &model_block.navigation_blocks {
            if let Some(nav) = self.nav(binding_symbol, model_block, nav, table, model_blocks) {
                navigation_properties.push(nav);
            }
        }

        // Build Column structs
        for field in &model_block.typed_idents {
            if !is_valid_sql_type(&field.cidl_type) {
                self.sink
                    .push(SemanticError::InvalidColumnType { column: field });
                continue;
            }

            let is_pk = model_block.primary_fields.contains(&field.name);
            if is_pk && field.cidl_type.is_nullable() {
                self.sink
                    .push(SemanticError::NullablePrimaryKey { column: field });
                continue;
            }

            let unique_ids_val = unique_info.remove(field.name).unwrap_or_default();

            let col = Column {
                hash: 0,
                field: Field {
                    name: field.name.into(),
                    cidl_type: field.cidl_type.clone(),
                },
                foreign_key_reference: None,
                unique_ids: unique_ids_val,
                composite_id: None,
            };

            if is_pk {
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

    fn nav(
        &mut self,
        binding_symbol: &'p Symbol<'src>,
        model_block: &'p ModelBlock<'src>,
        nav: &'p NavigationBlock<'src>,
        table: &mut SymbolTable<'src, 'p>,
        model_blocks: &BTreeMap<&'src str, &'p ModelBlock<'src>>,
    ) -> Option<NavigationField<'src>> {
        let model_name = model_block.symbol.name;

        // Validate all referenced fields exist on the same adj model
        let mut referenced_field_names = Vec::new();
        {
            let mut all_valid = true;
            let adj_model_name = nav.adj.first().map(|(m, _)| *m).unwrap_or("");
            for (ref_model_name, ref_field_name) in &nav.adj {
                if *ref_model_name != adj_model_name {
                    self.sink.push(SemanticError::InconsistentModelAdjacency {
                        span: nav.span,
                        first_model: adj_model_name,
                        second_model: ref_model_name,
                    });
                    all_valid = false;
                    continue;
                }

                if table
                    .resolve(ref_field_name, SymbolKind::ModelField, Some(ref_model_name))
                    .is_some()
                {
                    referenced_field_names.push(*ref_field_name);
                    continue;
                }

                self.sink.push(SemanticError::UnresolvedSymbol {
                    span: nav.span,
                    name: ref_field_name,
                });
                all_valid = false;
                continue;
            }
            if !all_valid {
                return None;
            }
        }

        let adj_model_block = model_blocks.get(nav.adj.first().unwrap().0).unwrap();

        // Must belong to the same database
        let adj_binding = adj_model_block
            .use_tag
            .as_ref()
            .and_then(|tag| tag.env_bindings.first().copied());
        if Some(binding_symbol.name) != adj_binding {
            self.sink
                .push(SemanticError::NavigationReferencesDifferentDatabase {
                    span: nav.span,
                    binding: adj_binding.unwrap_or("no binding"),
                });
            return None;
        }

        // For 1:1: check if `model` has a FK whose adj fields match nav.adj fields
        // (both sides reference the same adj model fields)
        let matching_fk_by_adj = |model: &'p ModelBlock<'src>, name: &'src str| {
            model.foreign_blocks.iter().find(|fb| {
                fb.adj.first().map(|(m, _)| *m == name).unwrap_or(false)
                    && fb.adj.len() == nav.adj.len()
                    && fb
                        .adj
                        .iter()
                        .zip(&nav.adj)
                        .all(|((_, adj_field), (_, nav_field))| adj_field == nav_field)
            })
        };

        // For 1:M: check if `model` has a FK pointing to `name` whose local fields match nav.adj field names
        // nav(Post::authorId) means Post's local field "authorId" is the FK column
        let matching_fk_by_local_fields = |model: &'p ModelBlock<'src>, name: &'src str| {
            model.foreign_blocks.iter().find(|fb| {
                fb.adj.first().map(|(m, _)| *m == name).unwrap_or(false)
                    && fb.fields.len() == nav.adj.len()
                    && fb
                        .fields
                        .iter()
                        .zip(&nav.adj)
                        .all(|(local_field, (_, nav_field))| local_field.name == *nav_field)
            })
        };

        if nav.is_one_to_one {
            // A foreign key must exist on this model that references the adjacent model
            // because of the parser syntax.
            let foreign_key = matching_fk_by_adj(model_block, adj_model_block.symbol.name).unwrap();

            // Foreign key has already been validated for 1:1 navs
            return Some(NavigationField {
                hash: 0,
                field: Field {
                    name: nav.field.name.into(),
                    cidl_type: CidlType::Object {
                        name: adj_model_block.symbol.name,
                    },
                },
                model_reference: adj_model_block.symbol.name,
                kind: NavigationFieldKind::OneToOne {
                    columns: foreign_key
                        .fields
                        .iter()
                        .map(|f| f.name)
                        .collect::<Vec<_>>(),
                },
            });
        }

        if matching_fk_by_local_fields(adj_model_block, model_block.symbol.name).is_some() {
            // If the adjacent fields form a foreign key to this model, it's 1:M
            return Some(NavigationField {
                hash: 0,
                field: Field {
                    name: nav.field.name.into(),
                    cidl_type: CidlType::Array(Box::new(CidlType::Object {
                        name: adj_model_block.symbol.name,
                    })),
                },
                model_reference: adj_model_block.symbol.name,
                kind: NavigationFieldKind::OneToMany {
                    columns: referenced_field_names,
                },
            });
        }

        // If the adjacent model has a reciprocal nav that references back to this model, it's a Many:Many nav.
        // Each side references the other model's PK fields (which may differ in name for composite PKs),
        // so we only check that the reciprocal nav points back to the current model.
        let matching_reciprocal_nav_count = adj_model_block
            .navigation_blocks
            .iter()
            .filter(|adj_nav| {
                !adj_nav.is_one_to_one
                    && adj_nav
                        .adj
                        .first()
                        .map(|(m, _)| *m == model_name)
                        .unwrap_or(false)
            })
            .count();

        if matching_reciprocal_nav_count == 0 {
            self.sink
                .push(SemanticError::NavigationMissingReciprocalM2M { span: nav.span });
            return None;
        }

        if matching_reciprocal_nav_count > 1 {
            self.sink
                .push(SemanticError::NavigationAmbiguousM2M { span: nav.span });
            return None;
        }

        Some(NavigationField {
            hash: 0,
            field: Field {
                name: nav.field.name.into(),
                cidl_type: CidlType::Array(Box::new(CidlType::Object {
                    name: adj_model_block.symbol.name,
                })),
            },
            model_reference: adj_model_block.symbol.name,
            kind: NavigationFieldKind::ManyToMany,
        })
    }

    /// Validates and sets all KV/R2-related properties of a model
    fn kv_r2_properties(
        &mut self,
        model: &mut Model<'src>,
        model_block: &'p ModelBlock<'src>,
        table: &SymbolTable<'src, 'p>,
    ) {
        // Validates that a KV/R2 tag's env binding exists and is of the correct WranglerEnvBindingKind
        let validate_binding = |sink: &mut ErrorSink<'src, 'p>,
                                env_binding: &'src str,
                                span: Span,
                                expected: WranglerEnvBindingKind|
         -> Option<&'src str> {
            if let Some(binding_sym) = table.resolve(
                env_binding,
                SymbolKind::WranglerEnvBinding {
                    kind: expected.clone(),
                },
                None,
            ) {
                return Some(binding_sym.name);
            }

            let err = match expected {
                WranglerEnvBindingKind::Kv => SemanticError::KvInvalidBinding {
                    span,
                    binding: env_binding,
                },
                WranglerEnvBindingKind::R2 => SemanticError::R2InvalidBinding {
                    span,
                    binding: env_binding,
                },
                _ => SemanticError::UnresolvedSymbol {
                    span,
                    name: env_binding,
                },
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
                    // Look through typed fields and key_fields for a matching name
                    let in_typed = model_block
                        .typed_idents
                        .iter()
                        .any(|f| f.name == var && is_valid_sql_type(&f.cidl_type));
                    let in_key_fields = model_block.key_fields.iter().any(|f| f.name == var);

                    if !in_typed && !in_key_fields {
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
            let binding_name = validate_binding(
                &mut self.sink,
                kv.env_binding,
                kv.span,
                WranglerEnvBindingKind::Kv,
            );

            if !validate_key_format(&mut self.sink, kv.span, kv.key_format) {
                continue;
            }

            let mut resolved_type = match resolve_cidl_type(&kv.field, &kv.field.cidl_type, table) {
                Ok(t) => t,
                Err(err) => {
                    self.sink.push(err);
                    continue;
                }
            };

            resolved_type = CidlType::KvObject(Box::new(resolved_type));

            if kv.is_paginated {
                resolved_type = CidlType::paginated(resolved_type)
            }

            model.kv_fields.push(KvR2Field {
                field: Field {
                    name: kv.field.name.into(),
                    cidl_type: resolved_type,
                },
                format: kv.key_format,
                binding: binding_name.unwrap_or_default(),
                list_prefix: false,
            });
        }

        for r2 in &model_block.r2s {
            let binding_name = validate_binding(
                &mut self.sink,
                r2.env_binding,
                r2.span,
                WranglerEnvBindingKind::R2,
            );

            if !validate_key_format(&mut self.sink, r2.span, r2.key_format) {
                continue;
            }

            model.r2_fields.push(KvR2Field {
                field: Field {
                    name: r2.field.name.into(),
                    cidl_type: if r2.is_paginated {
                        CidlType::paginated(CidlType::R2Object)
                    } else {
                        CidlType::R2Object
                    },
                },
                format: r2.key_format,
                binding: binding_name.unwrap_or_default(),
                list_prefix: false,
            });
        }

        for kf in &model_block.key_fields {
            model.key_fields.push(kf.name);
        }
    }
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
