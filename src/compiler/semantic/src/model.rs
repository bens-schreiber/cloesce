use crate::EnvBindingKind;
use crate::{
    LocalSymbolKind, SymbolTable, ensure,
    err::{BatchResult, ErrorSink, SemanticError},
    is_valid_sql_type, kahns, resolve_cidl_type, resolve_validators,
};
use ast::{
    CidlType, Column, CrudKind, Field, ForeignKeyReference, KvField, Model, NavigationField,
    NavigationFieldKind, R2Field, ValidatedField,
};
use frontend::{
    ForeignBlock, ForeignQualifier, KvBlock, ModelBlock, ModelBlockKind, PaginatedBlockKind,
    R2Block, SpdSlice, SqlBlockKind, Symbol,
};
use indexmap::IndexMap;
use std::{collections::BTreeMap, vec};

#[derive(Default)]
pub struct ModelAnalysis<'src, 'p> {
    sink: ErrorSink<'src, 'p>,
    in_degree: BTreeMap<&'src str, usize>,
    graph: BTreeMap<&'src str, Vec<&'src str>>,
}

impl<'src, 'p> ModelAnalysis<'src, 'p> {
    pub fn analyze(
        mut self,
        table: &SymbolTable<'src, 'p>,
    ) -> BatchResult<'src, 'p, IndexMap<&'src str, Model<'src>>> {
        let mut models: IndexMap<&'src str, Model<'src>> = IndexMap::new();

        for &model_block in table.models.values() {
            let (cruds, env_bindings) = model_block.partition_use_tags();

            let builder = ModelBuilder::new(model_block);
            let Some(mut model) = builder.build(&mut self, env_bindings, table) else {
                continue;
            };

            // Validate CRUD operations — List requires a D1 binding
            for crud in &cruds {
                ensure!(
                    !matches!(crud, CrudKind::List) || model.d1_binding.is_some(),
                    self.sink,
                    SemanticError::UnsupportedCrudOperation {
                        model: &model_block.symbol
                    }
                );
            }

            model.cruds = cruds.into_iter().cloned().collect();
            models.insert(model.name, model);
        }

        // Topologically sort models based on FK relationships
        match kahns(self.graph, self.in_degree, table.models.len()) {
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
}

struct ModelBuilder<'src, 'p> {
    name: &'src str,
    symbol: &'p Symbol<'src>,
    model: &'p ModelBlock<'src>,

    has_defined_pk: bool,
    unique_seed: usize,
    composite_seed: usize,
    primary_columns: Vec<Column<'src>>,
    columns: Vec<Column<'src>>,
    navigation_fields: Vec<NavigationField<'src>>,
    kv_fields: Vec<KvField<'src>>,
    r2_fields: Vec<R2Field<'src>>,
    key_fields: Vec<ValidatedField<'src>>,
}

impl<'src, 'p> ModelBuilder<'src, 'p> {
    pub fn new(model_block: &'p ModelBlock<'src>) -> Self {
        Self {
            name: model_block.symbol.name,
            symbol: &model_block.symbol,
            model: model_block,

            has_defined_pk: false,
            unique_seed: 0,
            composite_seed: 0,
            primary_columns: Vec::new(),
            columns: Vec::new(),
            navigation_fields: Vec::new(),
            kv_fields: Vec::new(),
            r2_fields: Vec::new(),
            key_fields: Vec::new(),
        }
    }
}

impl<'src, 'p> ModelBuilder<'src, 'p> {
    fn build(
        mut self,
        ma: &mut ModelAnalysis<'src, 'p>,
        env_bindings: Vec<&'p Symbol<'src>>,
        table: &SymbolTable<'src, 'p>,
    ) -> Option<Model<'src>> {
        ma.graph.entry(self.name).or_default();
        ma.in_degree.entry(self.name).or_insert(0);

        // Models with SQL columns require a D1 binding
        let has_sql_blocks = self.model.blocks.blocks().any(|b| {
            matches!(
                b,
                ModelBlockKind::Column(_)
                    | ModelBlockKind::Foreign(_)
                    | ModelBlockKind::Primary(_)
                    | ModelBlockKind::Unique(_)
                    | ModelBlockKind::Optional(_)
                    | ModelBlockKind::Navigation(_)
            )
        });

        // Resolve D1 binding if SQL content is present or an
        // environment bindings are present
        let binding_symbol = if has_sql_blocks || !env_bindings.is_empty() {
            if env_bindings.is_empty() {
                ma.sink
                    .push(SemanticError::D1ModelMissingD1Binding { model: self.symbol });
                return None;
            }

            if env_bindings.len() > 1 {
                ma.sink.push(SemanticError::D1ModelMultipleD1Bindings {
                    model: self.symbol,
                    bindings: env_bindings,
                });
                return None;
            }

            let binding_symbol = env_bindings[0];
            if !table.local.contains_key(&LocalSymbolKind::EnvBinding {
                kind: EnvBindingKind::D1,
                name: binding_symbol.name,
            }) {
                ma.sink.push(SemanticError::D1ModelInvalidD1Binding {
                    model: self.symbol,
                    binding: binding_symbol,
                });
                return None;
            };

            Some(binding_symbol)
        } else {
            None
        };

        for block in self.model.blocks.blocks() {
            match block {
                ModelBlockKind::Column(symbol) => {
                    self.column(ma, symbol, FieldQualifiers::default());
                }
                ModelBlockKind::Foreign(fk) => {
                    self.foreign(
                        ma,
                        table,
                        binding_symbol.unwrap(),
                        fk,
                        FieldQualifiers::default(),
                    );
                }
                ModelBlockKind::Primary(blocks) => {
                    let qual = FieldQualifiers {
                        is_primary: true,
                        ..Default::default()
                    };

                    for block in blocks {
                        match &block.block {
                            SqlBlockKind::Column(symbol) => {
                                self.column(ma, symbol, qual.clone());
                            }
                            SqlBlockKind::Foreign(foreign_block) => self.foreign(
                                ma,
                                table,
                                binding_symbol.unwrap(),
                                foreign_block,
                                qual.clone(),
                            ),
                        }
                    }
                }
                ModelBlockKind::Unique(blocks) => {
                    let qual = FieldQualifiers {
                        unique_ids: vec![self.unique_seed],
                        ..Default::default()
                    };
                    self.unique_seed += 1;

                    for block in blocks {
                        match &block.block {
                            SqlBlockKind::Column(symbol) => {
                                self.column(ma, symbol, qual.clone());
                            }
                            SqlBlockKind::Foreign(foreign_block) => self.foreign(
                                ma,
                                table,
                                binding_symbol.unwrap(),
                                foreign_block,
                                qual.clone(),
                            ),
                        }
                    }
                }
                ModelBlockKind::Optional(blocks) => {
                    let qual = FieldQualifiers {
                        is_optional: true,
                        ..Default::default()
                    };

                    for block in blocks {
                        match &block.block {
                            SqlBlockKind::Column(symbol) => {
                                self.column(ma, symbol, qual.clone());
                            }
                            SqlBlockKind::Foreign(foreign_block) => self.foreign(
                                ma,
                                table,
                                binding_symbol.unwrap(),
                                foreign_block,
                                qual.clone(),
                            ),
                        }
                    }
                }
                ModelBlockKind::Navigation(navigation_block) => self.nav(
                    ma,
                    binding_symbol.unwrap(),
                    &navigation_block.adj,
                    &navigation_block.nav.block,
                    false,
                    table,
                ),
                ModelBlockKind::Kv(kv_block) => {
                    self.kv_field(ma, table, kv_block);
                }
                ModelBlockKind::R2(r2_block) => {
                    self.r2_field(ma, table, r2_block);
                }
                ModelBlockKind::KeyField(fields) => {
                    for field in fields {
                        if !is_valid_sql_type(&field.cidl_type) {
                            ma.sink.push(SemanticError::KeyFieldInvalidType { field });
                            continue;
                        }

                        let validators = match resolve_validators(field) {
                            Ok(v) => v,
                            Err(errs) => {
                                ma.sink.extend(errs);
                                Vec::new()
                            }
                        };

                        self.key_fields.push(ValidatedField {
                            name: field.name.into(),
                            cidl_type: field.cidl_type.clone(),
                            validators,
                        });
                    }
                }
                ModelBlockKind::Paginated(blocks) => {
                    for block in blocks {
                        match &block.block {
                            PaginatedBlockKind::Kv(kv_block) => {
                                self.kv_field(ma, table, kv_block);
                            }
                            PaginatedBlockKind::R2(r2_block) => {
                                self.r2_field(ma, table, r2_block);
                            }
                        }
                    }
                }
            }
        }

        if binding_symbol.is_some() && !self.has_defined_pk {
            ma.sink
                .push(SemanticError::D1ModelMissingPrimaryKey { model: self.symbol });
            return None;
        }

        Some(Model {
            hash: 0,
            name: self.name,
            d1_binding: binding_symbol.map(|s| s.name),
            primary_columns: self.primary_columns,
            columns: self.columns,
            kv_fields: self.kv_fields,
            r2_fields: self.r2_fields,
            navigation_fields: self.navigation_fields,
            key_fields: self.key_fields,
            ..Default::default()
        })
    }

    fn column(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p>,
        symbol: &'p Symbol<'src>,
        qual: FieldQualifiers,
    ) {
        self.has_defined_pk |= qual.is_primary;
        let cidl_type = if qual.is_optional {
            CidlType::nullable(symbol.cidl_type.clone())
        } else {
            symbol.cidl_type.clone()
        };

        if !is_valid_sql_type(&cidl_type) {
            ma.sink
                .push(SemanticError::InvalidColumnType { column: symbol });
            return;
        }

        if qual.is_primary && cidl_type.is_nullable() {
            ma.sink
                .push(SemanticError::NullablePrimaryKey { column: symbol });
            return;
        }

        let validators = match resolve_validators(symbol) {
            Ok(v) => v,
            Err(errs) => {
                ma.sink.extend(errs);
                Vec::new()
            }
        };

        let col = Column {
            hash: 0,
            field: ValidatedField {
                name: symbol.name.into(),
                cidl_type,
                validators,
            },
            foreign_key_reference: None,
            unique_ids: qual.unique_ids,
            composite_id: None,
        };

        if qual.is_primary {
            self.primary_columns.push(col);
        } else {
            self.columns.push(col);
        }
    }

    fn foreign(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p>,
        table: &SymbolTable<'src, 'p>,
        binding_symbol: &'p Symbol<'src>,
        fk: &'p ForeignBlock<'src>,
        mut qual: FieldQualifiers,
    ) {
        // Add to qualifiers
        qual.is_optional |= fk.is_optional();
        qual.is_primary |= matches!(fk.qualifier, Some(ForeignQualifier::Primary));
        if matches!(fk.qualifier, Some(ForeignQualifier::Unique)) {
            qual.unique_ids.push(self.unique_seed);
            self.unique_seed += 1;
        }
        self.has_defined_pk |= qual.is_primary;

        // Check that the adjacent model exists
        let adj_model_sym = &fk.adj.first().unwrap().0;
        let Some(adj_model_block) = table.models.get(adj_model_sym.name) else {
            ma.sink.push(SemanticError::UnresolvedSymbol {
                symbol: adj_model_sym,
            });
            return;
        };

        if adj_model_sym.name == self.name {
            ma.sink.push(SemanticError::ForeignKeyReferencesSelf {
                model: self.symbol,
                foreign_key: adj_model_sym,
            });
            return;
        }

        // Must belong to the same database
        let adj_binding = adj_model_block.partition_use_tags().1.first().copied();
        if adj_binding.map(|s| s.name) != Some(binding_symbol.name) {
            ma.sink
                .push(SemanticError::ForeignKeyReferencesDifferentDatabase {
                    model: self.symbol,
                    fk_model: adj_model_sym,
                    fk_binding: adj_binding,
                });
            return;
        }

        // All adj entries must reference the same model
        if let Some((inconsistent_model, _)) =
            fk.adj.iter().find(|(m, _)| m.name != adj_model_sym.name)
        {
            ma.sink.push(SemanticError::InconsistentModelAdjacency {
                first_model: adj_model_sym,
                second_model: inconsistent_model,
            });
            return;
        }

        // Number of adj references must match number of local fields
        if fk.adj.len() != fk.fields.len() {
            ma.sink.push(SemanticError::ForeignKeyInconsistentFieldAdj {
                span: fk.adj.first().unwrap().0.span,
                adj_count: fk.adj.len(),
                field_count: fk.fields.len(),
            });
            return;
        }

        let composite_id = if fk.adj.len() > 1 {
            let id = self.composite_seed;
            self.composite_seed += 1;
            Some(id)
        } else {
            None
        };

        for (field, (_, adj_field_sym)) in fk.fields.iter().zip(&fk.adj) {
            if qual.is_primary && fk.is_optional() {
                ma.sink
                    .push(SemanticError::NullablePrimaryKey { column: field });
                continue;
            }

            // Validate the field from the adjacent model
            let Some(adj_field_sym) = table.local.get(&LocalSymbolKind::ModelField {
                model: adj_model_sym.name,
                name: adj_field_sym.name,
            }) else {
                ma.sink.push(SemanticError::UnresolvedSymbol {
                    symbol: adj_field_sym,
                });
                continue;
            };

            if !is_valid_sql_type(&adj_field_sym.cidl_type) {
                ma.sink.push(SemanticError::ForeignKeyInvalidColumnType {
                    field: adj_field_sym,
                });
            }

            if !fk.is_optional() {
                // One To One: Person has a Dog ..(sql)=> Person has a fk to Dog
                // Dog must come before Person
                ma.graph
                    .entry(adj_model_sym.name)
                    .or_default()
                    .push(self.name);
                *ma.in_degree.entry(self.name).or_insert(0) += 1;
            }

            // No reason to push these errors, it will be caught during
            // the validation of the adjacent model's own columns.
            let adj_validators = resolve_validators(adj_field_sym).unwrap_or_default();

            let col = Column {
                hash: 0,
                field: ValidatedField {
                    name: field.name.into(),
                    cidl_type: if fk.is_optional() {
                        CidlType::nullable(adj_field_sym.cidl_type.clone())
                    } else {
                        adj_field_sym.cidl_type.clone()
                    },
                    validators: adj_validators,
                },
                foreign_key_reference: Some(ForeignKeyReference {
                    model_name: adj_model_sym.name,
                    column_name: adj_field_sym.name,
                }),
                unique_ids: qual.unique_ids.clone(),
                composite_id,
            };

            if qual.is_primary {
                self.primary_columns.push(col);
            } else {
                self.columns.push(col);
            }
        }

        if let Some(nav_field) = &fk.nav {
            self.nav(
                ma,
                binding_symbol,
                &fk.adj,
                &nav_field.block.symbol,
                true,
                table,
            );
        }
    }

    fn nav(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p>,
        binding_symbol: &'p Symbol<'src>,
        adj: &'p [(Symbol<'src>, Symbol<'src>)],
        field: &'p Symbol<'src>,
        is_one_to_one: bool,
        table: &SymbolTable<'src, 'p>,
    ) {
        // Validate all referenced fields exist on the same adj model
        let mut referenced_field_names = Vec::new();
        {
            let mut all_valid = true;
            let adj_model_sym = &adj.first().unwrap().0;
            for (ref_model_sym, ref_field_sym) in adj {
                if ref_model_sym.name != adj_model_sym.name {
                    ma.sink.push(SemanticError::InconsistentModelAdjacency {
                        first_model: adj_model_sym,
                        second_model: ref_model_sym,
                    });
                    all_valid = false;
                    continue;
                }

                if table.local.contains_key(&LocalSymbolKind::ModelField {
                    model: adj_model_sym.name,
                    name: ref_field_sym.name,
                }) {
                    referenced_field_names.push(ref_field_sym.name);
                    continue;
                }

                ma.sink.push(SemanticError::UnresolvedSymbol {
                    symbol: ref_field_sym,
                });
                all_valid = false;
            }
            if !all_valid {
                return;
            }
        }

        let adj_model_block = table.models.get(adj.first().unwrap().0.name).unwrap();

        // Must belong to the same database
        let adj_binding = adj_model_block.partition_use_tags().1.first().copied();
        if adj_binding.map(|s| s.name) != Some(binding_symbol.name) {
            ma.sink
                .push(SemanticError::NavigationReferencesDifferentDatabase { field });
            return;
        }

        // For 1:1: check if `model` has a FK whose adj fields match the nav adj fields
        let matching_fk_by_adj = |model: &'p ModelBlock<'src>, name: &'src str| {
            model.foreign_blocks().find(|fb| {
                fb.adj.first().map(|(m, _)| m.name == name).unwrap_or(false)
                    && fb.adj.len() == adj.len()
                    && fb
                        .adj
                        .iter()
                        .zip(adj)
                        .all(|((_, fb_field), (_, nav_field))| fb_field.name == nav_field.name)
            })
        };

        // For 1:M: check if `model` has a FK pointing to `name` whose local fields match adj field names
        let matching_fk_by_local_fields = |model: &'p ModelBlock<'src>, name: &'src str| {
            model.foreign_blocks().find(|fb| {
                fb.adj.first().map(|(m, _)| m.name == name).unwrap_or(false)
                    && fb.fields.len() == adj.len()
                    && fb
                        .fields
                        .iter()
                        .zip(adj)
                        .all(|(local_field, (_, nav_field))| local_field.name == nav_field.name)
            })
        };

        if is_one_to_one {
            let foreign_key = matching_fk_by_adj(self.model, adj_model_block.symbol.name).unwrap();

            self.navigation_fields.push(NavigationField {
                hash: 0,
                field: Field {
                    name: field.name.into(),
                    cidl_type: CidlType::Object {
                        name: adj_model_block.symbol.name,
                    },
                },
                model_reference: adj_model_block.symbol.name,
                kind: NavigationFieldKind::OneToOne {
                    columns: foreign_key.fields.iter().map(|f| f.name).collect(),
                },
            });
            return;
        }

        if matching_fk_by_local_fields(adj_model_block, self.name).is_some() {
            self.navigation_fields.push(NavigationField {
                hash: 0,
                field: Field {
                    name: field.name.into(),
                    cidl_type: CidlType::Array(Box::new(CidlType::Object {
                        name: adj_model_block.symbol.name,
                    })),
                },
                model_reference: adj_model_block.symbol.name,
                kind: NavigationFieldKind::OneToMany {
                    columns: referenced_field_names,
                },
            });
            return;
        }

        // If the adjacent model has a reciprocal nav that references back to this model, it's a Many:Many nav.
        let matching_reciprocal_nav_count = adj_model_block
            .navigation_blocks()
            .filter(|adj_nav| {
                adj_nav
                    .adj
                    .first()
                    .map(|(m, _)| m.name == self.name)
                    .unwrap_or(false)
            })
            .count();

        if matching_reciprocal_nav_count == 0 {
            ma.sink
                .push(SemanticError::NavigationMissingReciprocalM2M { field });
            return;
        }

        if matching_reciprocal_nav_count > 1 {
            ma.sink
                .push(SemanticError::NavigationAmbiguousM2M { field });
            return;
        }

        self.navigation_fields.push(NavigationField {
            hash: 0,
            field: Field {
                name: field.name.into(),
                cidl_type: CidlType::Array(Box::new(CidlType::Object {
                    name: adj_model_block.symbol.name,
                })),
            },
            model_reference: adj_model_block.symbol.name,
            kind: NavigationFieldKind::ManyToMany,
        })
    }

    fn kv_field(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p>,
        table: &SymbolTable<'src, 'p>,
        kv: &'p KvBlock<'src>,
    ) {
        let binding_name =
            Self::validate_binding(&mut ma.sink, table, &kv.env_binding, EnvBindingKind::Kv);

        let Ok(format_parameters) =
            Self::validate_key_format(&mut ma.sink, self.model, &kv.field, kv.key_format)
        else {
            return;
        };

        let mut resolved_type = match resolve_cidl_type(&kv.field, &kv.field.cidl_type, table) {
            Ok(t) => t,
            Err(err) => {
                ma.sink.push(err);
                return;
            }
        };

        let validators = match resolve_validators(&kv.field) {
            Ok(v) => v,
            Err(errs) => {
                ma.sink.extend(errs);
                Vec::new()
            }
        };

        resolved_type = CidlType::KvObject(Box::new(resolved_type));

        if kv.is_paginated {
            resolved_type = CidlType::paginated(resolved_type)
        }

        self.kv_fields.push(KvField {
            field: ValidatedField {
                name: kv.field.name.into(),
                cidl_type: resolved_type,
                validators,
            },
            format: kv.key_format,
            binding: binding_name.unwrap_or_default(),
            format_parameters,
            list_prefix: kv.is_paginated,
        });
    }

    fn r2_field(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p>,
        table: &SymbolTable<'src, 'p>,
        r2: &'p R2Block<'src>,
    ) {
        let binding_name =
            Self::validate_binding(&mut ma.sink, table, &r2.env_binding, EnvBindingKind::R2);

        let Ok(format_parameters) =
            Self::validate_key_format(&mut ma.sink, self.model, &r2.field, r2.key_format)
        else {
            return;
        };

        self.r2_fields.push(R2Field {
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
            format_parameters,
            list_prefix: r2.is_paginated,
        });
    }

    // Validates that a KV/R2 tag's env binding exists and is of the correct WranglerEnvBindingKind
    fn validate_binding(
        sink: &mut ErrorSink<'src, 'p>,
        table: &SymbolTable<'src, 'p>,
        env_binding: &'p Symbol<'src>,
        expected: EnvBindingKind,
    ) -> Option<&'src str> {
        if let Some(binding_sym) = table.local.get(&LocalSymbolKind::EnvBinding {
            kind: expected.clone(),
            name: env_binding.name,
        }) {
            return Some(binding_sym.name);
        }

        let err = match expected {
            EnvBindingKind::Kv => SemanticError::KvInvalidBinding {
                binding: env_binding,
            },
            EnvBindingKind::R2 => SemanticError::R2InvalidBinding {
                binding: env_binding,
            },
            _ => SemanticError::UnresolvedSymbol {
                symbol: env_binding,
            },
        };
        sink.push(err);
        None
    }

    // Extracts variables from a formatted string, then validates that they
    // correspond to fields on the model that are of valid SQLite types or are key_fields.
    // Returns the parameters to create a key format.
    fn validate_key_format(
        sink: &mut ErrorSink<'src, 'p>,
        model_block: &'p ModelBlock<'src>,
        field: &'p Symbol<'src>,
        format: &'src str,
    ) -> Result<Vec<Field<'src>>, ()> {
        let vars = match extract_braced(format) {
            Ok(vars) => vars,
            Err(reason) => {
                sink.push(SemanticError::KvR2InvalidKeyFormat { field, reason });
                return Err(());
            }
        };

        let key_field_names: Vec<&'src str> = model_block
            .blocks
            .blocks()
            .flat_map(|b| match b {
                ModelBlockKind::KeyField(fields) => {
                    fields.iter().map(|f| f.name).collect::<Vec<_>>()
                }
                _ => vec![],
            })
            .collect();

        let mut parameters = vec![];
        for var in vars {
            let column = model_block
                .sql_symbols()
                .find(|f| f.name == var && is_valid_sql_type(&f.cidl_type));
            let is_key_field = key_field_names.contains(&var);

            match (column, is_key_field) {
                (Some(col), _) => parameters.push(Field {
                    name: col.name.into(),
                    cidl_type: col.cidl_type.clone(),
                }),
                (None, true) => parameters.push(Field {
                    name: var.into(),
                    cidl_type: CidlType::String,
                }),
                (None, false) => {
                    sink.push(SemanticError::KvR2UnknownKeyVariable {
                        field,
                        variable: var,
                    });
                    return Err(());
                }
            }
        }

        Ok(parameters)
    }
}

#[derive(Default, Clone)]
struct FieldQualifiers {
    is_optional: bool,
    is_primary: bool,
    unique_ids: Vec<usize>,
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
