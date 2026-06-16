use crate::{
    LocalSymbolKind, SymbolTable, ensure,
    err::{BatchResult, ErrorSink, SemanticError},
    is_valid_sql_type, kahns, resolve_cidl_type, resolve_validator_tags,
};
use frontend::{
    ForeignBlock, KvFieldBlock, ModelBlock, ModelBlockKind, NavAdj, R2FieldBlock, SpdSlice,
    SqlBlockKind, Symbol, Tag,
};
use idl::{
    BackingKind, BindingTemplate, CidlType, Column, CrudKind, Field, ForeignKeyReference, KvField,
    Model, ModelBacking, NavigationField, NavigationFieldKind, R2Field, ValidatedField,
    WranglerEnv,
};
use indexmap::IndexMap;
use std::collections::HashSet;
use std::{collections::BTreeMap, ops::Not};

pub struct ModelAnalysis<'src, 'p, 'sem> {
    env: &'sem WranglerEnv<'src>,
    sink: ErrorSink<'src, 'p>,
    in_degree: BTreeMap<&'src str, usize>,
    graph: BTreeMap<&'src str, Vec<&'src str>>,
}

impl<'src, 'p, 'sem> ModelAnalysis<'src, 'p, 'sem> {
    pub fn new(env: &'sem WranglerEnv<'src>) -> Self {
        Self {
            env,
            sink: ErrorSink::new(),
            in_degree: BTreeMap::new(),
            graph: BTreeMap::new(),
        }
    }

    pub fn analyze(
        mut self,
        table: &SymbolTable<'src, 'p>,
    ) -> BatchResult<'src, 'p, IndexMap<&'src str, Model<'src>>> {
        let mut models: IndexMap<&'src str, Model<'src>> = IndexMap::new();

        for &model_block in table.models.values() {
            // Validate tags
            let mut dedup_cruds = HashSet::new();
            let mut cruds = Vec::new();
            for tag in &model_block.symbol.tags {
                match &tag.inner {
                    Tag::Crud { kinds } => {
                        for kind in kinds {
                            if dedup_cruds.insert(kind.inner.clone()) {
                                cruds.push(kind);
                            }
                        }
                    }
                    _ => self.sink.push(SemanticError::TagInvalidInContext {
                        tag,
                        symbol: &model_block.symbol,
                    }),
                }
            }

            let builder = ModelBuilder::new(model_block);
            let Some(mut model) = builder.build(&mut self, table) else {
                continue;
            };

            // `list` needs a SQL store
            for crud in &cruds {
                ensure!(
                    !matches!(crud.inner, CrudKind::List) || model.uses_sqlite(),
                    self.sink,
                    SemanticError::UnsupportedCrudOperation {
                        model: &model_block.symbol,
                        crud,
                    }
                );
            }

            model.cruds = cruds.into_iter().map(|c| c.inner.clone()).collect();
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

    unique_seed: usize,
    composite_seed: usize,
    primary_columns: Vec<Column<'src>>,
    columns: Vec<Column<'src>>,
    navigation_fields: Vec<NavigationField<'src>>,
    kv_fields: Vec<KvField<'src>>,
    r2_fields: Vec<R2Field<'src>>,
    route_fields: Vec<ValidatedField<'src>>,
    backing: Option<ModelBacking<'src>>,
}

impl<'src, 'p> ModelBuilder<'src, 'p> {
    pub fn new(model_block: &'p ModelBlock<'src>) -> Self {
        Self {
            name: model_block.symbol.name,
            symbol: &model_block.symbol,
            model: model_block,

            unique_seed: 0,
            composite_seed: 0,
            primary_columns: Vec::new(),
            columns: Vec::new(),
            navigation_fields: Vec::new(),
            kv_fields: Vec::new(),
            r2_fields: Vec::new(),
            route_fields: Vec::new(),
            backing: None,
        }
    }
}

impl<'src, 'p, 'sem> ModelBuilder<'src, 'p> {
    fn build(
        mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        table: &SymbolTable<'src, 'p>,
    ) -> Option<Model<'src>> {
        ma.graph.entry(self.name).or_default();
        ma.in_degree.entry(self.name).or_insert(0);

        let has_sql_blocks = self.model.blocks.inners().any(|b| {
            matches!(
                b,
                ModelBlockKind::Column(_)
                    | ModelBlockKind::Foreign(_)
                    | ModelBlockKind::Primary(_)
                    | ModelBlockKind::Unique(_)
            )
        });

        let has_route_blocks = self
            .model
            .blocks
            .inners()
            .any(|b| matches!(b, ModelBlockKind::Route(_)));

        let binding = self.model.database_binding.as_ref();
        if binding.is_none() && has_sql_blocks {
            // A model with SQL blocks must specify a database binding
            ma.sink
                .push(SemanticError::ModelMissingDatabaseBinding { model: self.symbol });
            return None;
        }

        // Resolve backing
        if let Some(binding_sym) = binding {
            let is_durable = table.durable_bindings.contains_key(binding_sym.name);
            let is_d1 = table
                .d1_bindings
                .iter()
                .flat_map(|b| b.bindings.iter())
                .any(|s| s.name == binding_sym.name);

            if !is_durable && !is_d1 {
                // A model can't be backed by non DO/ D1 bindings
                ma.sink.push(SemanticError::ModelInvalidBinding {
                    model: self.symbol,
                    binding: binding_sym,
                });
                return None;
            }

            let kind = if is_durable {
                BackingKind::DurableObject
            } else {
                BackingKind::D1
            };

            self.backing(ma, table, binding_sym, kind);
        }

        let is_d1_backed = matches!(
            self.backing.as_ref().map(|b| &b.kind),
            Some(BackingKind::D1)
        );
        let needs_pk = has_sql_blocks || is_d1_backed;

        if has_route_blocks && needs_pk {
            ma.sink
                .push(SemanticError::ModelMixesRoutesAndSql { model: self.symbol });
            return None;
        }

        for block in self.model.blocks.inners() {
            match block {
                ModelBlockKind::Column(symbols) => {
                    for symbol in symbols {
                        self.column(ma, symbol, false);
                    }
                }
                ModelBlockKind::Foreign(fk) => {
                    self.foreign(ma, table, binding.unwrap().name, fk, false);
                }
                ModelBlockKind::Primary(blocks) => {
                    for block in blocks {
                        match &block.inner {
                            SqlBlockKind::Column(symbol) => {
                                self.column(ma, symbol, true);
                            }
                            SqlBlockKind::Foreign(foreign_block) => {
                                self.foreign(ma, table, binding.unwrap().name, foreign_block, true)
                            }
                        }
                    }
                }
                ModelBlockKind::Navigation(navigation_block) => self.nav(
                    ma,
                    binding,
                    &navigation_block.adj,
                    &navigation_block.nav.inner,
                    table,
                ),
                ModelBlockKind::Route(symbols) => {
                    for symbol in symbols {
                        self.route_field(ma, symbol);
                    }
                }
                ModelBlockKind::Unique(_) | ModelBlockKind::Kv(_) | ModelBlockKind::R2(_) => {
                    // Processed once all columns are built
                }
            }
        }

        for block in self.model.blocks.inners() {
            match block {
                ModelBlockKind::Unique(fields) => self.unique_constraint(ma, fields),
                ModelBlockKind::Kv(kv) => self.kv_field(ma, table, kv, binding),
                ModelBlockKind::R2(r2) => self.r2_field(ma, table, r2),
                _ => {}
            }
        }

        if needs_pk && self.primary_columns.is_empty() {
            ma.sink
                .push(SemanticError::ModelMissingPrimaryKey { model: self.symbol });
            return None;
        }

        Some(Model {
            name: self.name,
            backing: self.backing,
            primary_columns: self.primary_columns,
            columns: self.columns,
            kv_fields: self.kv_fields,
            r2_fields: self.r2_fields,
            navigation_fields: self.navigation_fields,
            route_fields: self.route_fields,
            ..Default::default()
        })
    }

    /// Resolves the [Model::backing] as well as expanding a Durable Object's shard
    /// fields into [Model::route_fields] if [BackingKind::DurableObject].
    fn backing(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        table: &SymbolTable<'src, 'p>,
        binding_sym: &'p Symbol<'src>,
        kind: BackingKind,
    ) {
        if matches!(kind, BackingKind::D1) {
            // Shard args are only meaningful for a Durable Object backing.
            if self.model.shard_args.is_some() {
                ma.sink.push(SemanticError::ModelInvalidBinding {
                    model: self.symbol,
                    binding: binding_sym,
                });
                return;
            }
            self.backing = Some(ModelBacking {
                binding: binding_sym.name,
                fields: Vec::new(),
                kind: BackingKind::D1,
            });
            return;
        }

        let shard_fields = {
            let block = table.durable_bindings.get(binding_sym.name).unwrap();
            block
                .shard_blocks
                .inners()
                .flat_map(|s| &s.fields)
                .collect::<Vec<_>>()
        };

        let shard_args = self.model.shard_args.as_deref().unwrap_or(&[]);
        if shard_args.len() != shard_fields.len() {
            ma.sink.push(SemanticError::ArgCountMismatch {
                field: binding_sym,
                expected: shard_fields.len(),
                got: shard_args.len(),
            });
            return;
        }

        let mut shard_field_names = Vec::with_capacity(shard_fields.len());
        for (arg, shard_field) in shard_args.iter().zip(&shard_fields) {
            let cidl_type = match resolve_cidl_type(shard_field, &shard_field.cidl_type, table) {
                Ok(t) => t,
                Err(e) => {
                    ma.sink.push(e);
                    continue;
                }
            };
            let validators = match resolve_validator_tags(shard_field) {
                Ok(v) => v,
                Err(errs) => {
                    ma.sink.extend(errs);
                    Vec::new()
                }
            };

            self.route_fields.push(ValidatedField {
                name: arg.name.into(),
                cidl_type,
                validators,
            });
            shard_field_names.push(arg.name);
        }

        self.backing = Some(ModelBacking {
            binding: binding_sym.name,
            fields: shard_field_names,
            kind: BackingKind::DurableObject,
        });
    }

    fn column(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        symbol: &'p Symbol<'src>,
        is_primary: bool,
    ) {
        let cidl_type = symbol.cidl_type.clone();

        if !is_valid_sql_type(&cidl_type) {
            ma.sink
                .push(SemanticError::InvalidColumnType { column: symbol });
            return;
        }

        if is_primary && cidl_type.is_nullable() {
            ma.sink
                .push(SemanticError::NullablePrimaryKey { column: symbol });
            return;
        }

        // Validate tags
        for tag in &symbol.tags {
            if !matches!(tag.inner, Tag::Validator { .. }) {
                ma.sink
                    .push(SemanticError::TagInvalidInContext { tag, symbol });
            }
        }
        let validators = match resolve_validator_tags(symbol) {
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
            unique_ids: Vec::new(),
            composite_id: None,
        };

        if is_primary {
            self.primary_columns.push(col);
        } else {
            self.columns.push(col);
        }
    }

    fn route_field(&mut self, ma: &mut ModelAnalysis<'src, 'p, 'sem>, symbol: &'p Symbol<'src>) {
        let cidl_type = symbol.cidl_type.clone();

        if !is_valid_sql_type(&cidl_type) {
            ma.sink
                .push(SemanticError::InvalidColumnType { column: symbol });
            return;
        }

        for tag in &symbol.tags {
            if !matches!(tag.inner, Tag::Validator { .. }) {
                ma.sink
                    .push(SemanticError::TagInvalidInContext { tag, symbol });
            }
        }
        let validators = match resolve_validator_tags(symbol) {
            Ok(v) => v,
            Err(errs) => {
                ma.sink.extend(errs);
                Vec::new()
            }
        };

        self.route_fields.push(ValidatedField {
            name: symbol.name.into(),
            cidl_type,
            validators,
        });
    }

    fn foreign(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        table: &SymbolTable<'src, 'p>,
        binding: &'src str,
        fk: &'p ForeignBlock<'src>,
        is_primary: bool,
    ) {
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
        let adj_binding = adj_model_block.database_binding.as_ref();
        if adj_binding.map(|s| s.name) != Some(binding) {
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
            if is_primary && fk.is_optional {
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

            if !fk.is_optional {
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
            let adj_validators = resolve_validator_tags(adj_field_sym).unwrap_or_default();

            let col = Column {
                hash: 0,
                field: ValidatedField {
                    name: field.name.into(),
                    cidl_type: if fk.is_optional {
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
                unique_ids: Vec::new(),
                composite_id,
            };

            if is_primary {
                self.primary_columns.push(col);
            } else {
                self.columns.push(col);
            }
        }
    }

    fn nav(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        binding: Option<&'p Symbol<'src>>,
        adj: &'p [NavAdj<'src>],
        field: &'p Symbol<'src>,
        table: &SymbolTable<'src, 'p>,
    ) {
        // 1:1 and 1:M entries cannot mix.
        let keyed = adj.first().map(|a| a.local_key.is_some()).unwrap_or(false);
        if adj.iter().any(|a| a.local_key.is_some() != keyed) {
            ma.sink
                .push(SemanticError::NavigationMixedAdjacency { field });
            return;
        }

        // Validate all referenced fields exist on the same adj model.
        let mut referenced_field_names = Vec::new();
        let adj_model_sym = &adj.first().unwrap().model;
        for entry in adj {
            if entry.model.name != adj_model_sym.name {
                ma.sink.push(SemanticError::InconsistentModelAdjacency {
                    first_model: adj_model_sym,
                    second_model: &entry.model,
                });
                return;
            }

            let Some(entry_field) = entry.field.as_ref() else {
                continue;
            };
            if table.local.contains_key(&LocalSymbolKind::ModelField {
                model: adj_model_sym.name,
                name: entry_field.name,
            }) {
                referenced_field_names.push(entry_field.name);
                continue;
            }

            ma.sink.push(SemanticError::UnresolvedSymbol {
                symbol: entry_field,
            });
            return;
        }

        let adj_model_block = table.models.get(adj.first().unwrap().model.name).unwrap();

        // If a model has no primary key columns, it cannot be the target of a SQL-backed navigation,
        // and must be worker backed.
        let adj_is_worker_backed = adj_model_block
            .blocks
            .inners()
            .any(|b| matches!(b, ModelBlockKind::Primary(_)))
            .not();
        if adj_is_worker_backed {
            let target_route_fields = adj_model_block.blocks.inners().find_map(|b| match b {
                ModelBlockKind::Route(symbols) => Some(symbols),
                _ => None,
            });

            if !keyed {
                if target_route_fields.map(|r| !r.is_empty()).unwrap_or(false) {
                    ma.sink.push(SemanticError::RouteNavigationInvalid {
                        field,
                        reason: "route navigations must be 1:1; declare the local key, e.g. `nav T::f(localKey)`",
                    });
                    return;
                }
                // Keyless singleton: target has no primary key and no route fields.
                self.navigation_fields.push(NavigationField {
                    hash: 0,
                    field: Field {
                        name: field.name.into(),
                        cidl_type: CidlType::Object {
                            name: adj_model_block.symbol.name,
                        },
                    },
                    model_reference: adj_model_block.symbol.name,
                    kind: NavigationFieldKind::OneToOne { fields: vec![] },
                });
                return;
            }

            // Each adj entry maps a target route field to one of this model's local fields.
            for entry in adj {
                let local_key = entry.local_key.as_ref().unwrap();
                let entry_field = entry.field.as_ref().unwrap();

                let Some(local_field) = table.local.get(&LocalSymbolKind::ModelField {
                    model: self.name,
                    name: local_key.name,
                }) else {
                    ma.sink
                        .push(SemanticError::UnresolvedSymbol { symbol: local_key });
                    return;
                };
                let adj_field = table
                    .local
                    .get(&LocalSymbolKind::ModelField {
                        model: adj_model_block.symbol.name,
                        name: entry_field.name,
                    })
                    .unwrap();

                if local_field.cidl_type != adj_field.cidl_type {
                    ma.sink.push(SemanticError::RouteNavigationInvalid {
                    field,
                    reason: "local route field type does not match the referenced route field type",
                });
                    return;
                }
            }

            // The target must have all of its route fields supplied
            let mut fields = Vec::new();
            for route_field in target_route_fields.into_iter().flatten() {
                let Some(entry) = adj
                    .iter()
                    .find(|a| a.field.as_ref().map(|f| f.name) == Some(route_field.name))
                else {
                    ma.sink.push(SemanticError::RouteNavigationInvalid {
                        field,
                        reason: "a route navigation must supply every route field of the target model",
                    });
                    return;
                };
                fields.push(entry.local_key.as_ref().unwrap().name);
            }

            self.navigation_fields.push(NavigationField {
                hash: 0,
                field: Field {
                    name: field.name.into(),
                    cidl_type: CidlType::Object {
                        name: adj_model_block.symbol.name,
                    },
                },
                model_reference: adj_model_block.symbol.name,
                kind: NavigationFieldKind::OneToOne { fields },
            });
            return;
        }

        // Otherwise, the adjacent model is D1 backed, and navigations to it must
        // reference the same D1 binding
        let adj_binding = adj_model_block.database_binding.as_ref().map(|s| s.name);
        if adj_binding != binding.map(|s| s.name) {
            ma.sink
                .push(SemanticError::NavigationReferencesDifferentBacking { field });
            return;
        }

        // A nav is 1:1 iff it carries local keys
        if keyed {
            let local_keys = adj
                .iter()
                .map(|a| a.local_key.as_ref().unwrap())
                .collect::<Vec<_>>();

            let foreign_key = self.model.foreign_blocks().find(|fb| {
                let references_adj_model = fb
                    .adj
                    .first()
                    .map(|(m, _)| m.name == adj_model_block.symbol.name)
                    .unwrap_or(false);

                let adj_fields_match = fb.adj.len() == adj.len()
                    && fb.adj.iter().zip(adj).all(|((_, fb_field), nav)| {
                        fb_field.name == nav.field.as_ref().unwrap().name
                    });

                let local_keys_match = fb.fields.len() == local_keys.len()
                    && fb
                        .fields
                        .iter()
                        .zip(&local_keys)
                        .all(|(fk_local, declared)| fk_local.name == declared.name);

                references_adj_model && adj_fields_match && local_keys_match
            });

            let Some(foreign_key) = foreign_key else {
                ma.sink.push(SemanticError::NavigationMissingForeignKey {
                    field,
                    model_reference: adj_model_block.symbol.name,
                });
                return;
            };

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
                    fields: foreign_key.fields.iter().map(|f| f.name).collect(),
                },
            });
            return;
        }

        // For 1:M: check if `model` has a FK pointing to `name` whose local fields match
        // adj field names
        let matching_fk_by_local_fields = adj_model_block
            .foreign_blocks()
            .find(|fb| {
                let references_model = fb
                    .adj
                    .first()
                    .map(|(m, _)| m.name == self.name)
                    .unwrap_or(false);

                let local_fields_match = fb.fields.len() == adj.len()
                    && fb.fields.iter().zip(adj).all(|(local_field, nav)| {
                        local_field.name == nav.field.as_ref().unwrap().name
                    });

                references_model && local_fields_match
            })
            .is_none();

        if matching_fk_by_local_fields {
            ma.sink.push(SemanticError::NavigationMissingForeignKey {
                field,
                model_reference: adj_model_block.symbol.name,
            });
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
            kind: NavigationFieldKind::OneToMany {
                columns: referenced_field_names,
            },
        });
    }

    // NOTE: Ran after all columns are processed
    fn unique_constraint(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        fields: &'p [Symbol<'src>],
    ) {
        let mut targets: Vec<usize> = Vec::new();
        for field in fields {
            if self
                .primary_columns
                .iter()
                .any(|c| c.field.name == field.name)
            {
                // References to primary-key columns can just be
                // dropped, since the PK is already unique.
                continue;
            }
            match self.columns.iter().position(|c| c.field.name == field.name) {
                Some(i) => targets.push(i),
                None => ma
                    .sink
                    .push(SemanticError::UnresolvedSymbol { symbol: field }),
            }
        }

        if targets.is_empty() {
            return;
        }

        let id = self.unique_seed;
        self.unique_seed += 1;
        for idx in targets {
            self.columns[idx].unique_ids.push(id);
        }
    }

    /// NOTE: Ran after all columns are processed
    fn kv_field(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        table: &SymbolTable<'src, 'p>,
        kv: &'p KvFieldBlock<'src>,
        binding: Option<&'p Symbol<'src>>,
    ) {
        if !table.kv_bindings.contains_key(kv.binding.name)
            && !table.durable_bindings.contains_key(kv.binding.name)
        {
            // KV must be either a Wrangler KV binding or a Durable Object binding
            ma.sink.push(SemanticError::UnresolvedSymbol {
                symbol: &kv.binding,
            });
            return;
        }

        let durable_kv_templates = ma
            .env
            .durable_bindings
            .iter()
            .find(|b| b.name == kv.binding.name)
            .map(|b| b.templates.as_slice());

        let kv_templates = ma
            .env
            .kv_bindings
            .iter()
            .find(|b| b.name == kv.binding.name)
            .map(|b| b.templates.as_slice());

        let templates = match (kv_templates, durable_kv_templates) {
            (Some(kv), _) => kv,
            (_, Some(durable)) => {
                if binding.map(|b| b.name) != Some(kv.binding.name) {
                    ma.sink.push(SemanticError::UnresolvedSymbol {
                        symbol: &kv.binding,
                    });
                    return;
                }
                durable
            }
            (None, None) => {
                // The binding is invalid but the error has already been sunk during env validation
                return;
            }
        };

        let Some((template, key_format)) = self.resolve_binding_ref(
            ma,
            table,
            templates,
            &kv.binding,
            &kv.binding_template,
            &kv.args,
            &kv.field,
        ) else {
            return;
        };

        self.kv_fields.push(KvField {
            field: ValidatedField {
                name: kv.field.name.into(),
                cidl_type: template.field.cidl_type.clone(),
                validators: template.field.validators.clone(),
            },
            binding: kv.binding.name,
            key_format,
        });
    }

    /// NOTE: Ran after all columns are processed
    fn r2_field(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        table: &SymbolTable<'src, 'p>,
        r2: &'p R2FieldBlock<'src>,
    ) {
        if !table.r2_bindings.contains_key(&r2.binding.name) {
            ma.sink.push(SemanticError::UnresolvedSymbol {
                symbol: &r2.binding,
            });
            return;
        }

        let Some(templates) = ma
            .env
            .r2_bindings
            .iter()
            .find(|b| b.name == r2.binding.name)
            .map(|b| b.templates.as_slice())
        else {
            // The binding is invalid but the error has already been sunk during env validation
            return;
        };

        let Some((template, key_format)) = self.resolve_binding_ref(
            ma,
            table,
            templates,
            &r2.binding,
            &r2.binding_template,
            &r2.args,
            &r2.field,
        ) else {
            return;
        };

        self.r2_fields.push(R2Field {
            field: Field {
                name: r2.field.name.into(),
                cidl_type: template.field.cidl_type.clone(),
            },
            binding: r2.binding.name,
            key_format,
        });
    }

    /// Resolves a KV/R2/Durable storage binding reference against the supplied
    /// templates and validates its args against the matched template's params.
    ///
    /// On success, returns the resolved binding template and the key format with
    /// the binding template's param placeholders replaced by the model's field
    /// names.
    ///
    /// Extends each referenced column with the validators
    /// declared on the corresponding binding template param.
    #[allow(clippy::too_many_arguments)]
    #[inline(always)]
    fn resolve_binding_ref(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        table: &SymbolTable<'src, 'p>,
        templates: &'sem [BindingTemplate<'src>],
        binding_sym: &'p Symbol<'src>,
        template_sym: &'p Symbol<'src>,
        args: &'p [Symbol<'src>],
        field: &'p Symbol<'src>,
    ) -> Option<(&'sem BindingTemplate<'src>, String)> {
        if !table.local.contains_key(&LocalSymbolKind::BindingTemplate {
            binding: binding_sym.name,
            name: template_sym.name,
        }) {
            ma.sink.push(SemanticError::UnresolvedSymbol {
                symbol: template_sym,
            });
            return None;
        }

        let Some(wrangler_binding_template) =
            templates.iter().find(|f| f.field.name == template_sym.name)
        else {
            // The binding is invalid but the error has already been sunk during env validation
            return None;
        };

        if args.len() != wrangler_binding_template.params.len() {
            ma.sink.push(SemanticError::ArgCountMismatch {
                field,
                expected: wrangler_binding_template.params.len(),
                got: args.len(),
            });
            return None;
        }

        // Each arg must reference a field on this model of the same type as
        // the corresponding binding template param.
        let mut key_format = wrangler_binding_template.key_format.to_string();
        for (arg, param) in args.iter().zip(&wrangler_binding_template.params) {
            if !table.local.contains_key(&LocalSymbolKind::ModelField {
                model: self.name,
                name: arg.name,
            }) {
                ma.sink
                    .push(SemanticError::UnresolvedSymbol { symbol: arg });
                return None;
            }

            let Some(arg_field) = self
                .primary_columns
                .iter_mut()
                .chain(&mut self.columns)
                .map(|c| &mut c.field)
                .chain(&mut self.route_fields)
                .find(|f| f.name == arg.name)
            else {
                // The referenced symbol exists on the model but is not a column or route field.
                ma.sink.push(SemanticError::ArgTypeMismatch { field, arg });
                return None;
            };

            if arg_field.cidl_type != param.cidl_type {
                ma.sink.push(SemanticError::ArgTypeMismatch { field, arg });
                return None;
            }

            // Inherit validators from the binding template param onto the model's field
            arg_field.validators.extend(param.validators.clone());

            // Replace the `{param}` placeholder with `{arg}`
            key_format =
                key_format.replace(&format!("{{{}}}", param.name), &format!("{{{}}}", arg.name));
        }

        Some((wrangler_binding_template, key_format))
    }
}
