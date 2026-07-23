use crate::{
    LocalSymbolKind, SymbolTable,
    err::{BatchResult, ErrorSink, SemanticError},
    is_valid_sql_type, resolve_cidl_type, resolve_validator_tags,
};
use frontend::{
    Cardinality, ForeignBlock, KvFieldArgument, KvFieldBlock, ModelBlock, ModelBlockKind,
    NavigationBlock, R2FieldBlock, SpdSlice, SqlBlockKind, Symbol, Tag,
};
use idl::{
    BackingKind, BindingTemplate, CidlType, Column, Field, ForeignKeyReference, KvField, Model,
    ModelBacking, NavigationCardinality, NavigationField, NavigationKeyMapping, R2Field,
    TemplateSegment, ValidatedField, WranglerEnv,
};
use indexmap::IndexMap;
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

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
                    Tag::Unique { .. } => {
                        if model_block.database_binding.is_none() {
                            self.sink.push(SemanticError::TagInvalidInContext {
                                tag,
                                symbol: &model_block.symbol,
                            });
                        }

                        // Unique constraints are validated in the ModelBuilder
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
                ModelBlockKind::Column(_) | ModelBlockKind::Foreign(_) | ModelBlockKind::Primary(_)
            )
        });

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
                ModelBlockKind::Navigation(navigation_block) => {
                    self.nav(ma, navigation_block, table)
                }
                ModelBlockKind::Route(symbols) => {
                    for symbol in symbols {
                        self.route_field(ma, symbol);
                    }
                }
                ModelBlockKind::Kv(_) | ModelBlockKind::R2(_) => {
                    // Processed once all columns are built
                }
            }
        }

        for block in self.model.blocks.inners() {
            match block {
                ModelBlockKind::Kv(kv) => self.kv_field(ma, table, kv),
                ModelBlockKind::R2(r2) => self.r2_field(ma, table, r2),
                _ => {}
            }
        }

        for tag in &self.model.symbol.tags {
            let Tag::Unique { fields: symbols } = &tag.inner else {
                continue;
            };
            self.unique_constraint(ma, symbols);
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
        let adj_model_sym = &fk.model;
        let Some(adj_model_block) = table.models.get(adj_model_sym.name) else {
            ma.sink.push(SemanticError::UnresolvedSymbol {
                symbol: adj_model_sym,
            });
            return;
        };

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

        // Number of target references must match number of local fields
        if fk.targets.len() != fk.fields.len() {
            ma.sink.push(SemanticError::ForeignKeyInconsistentFieldAdj {
                span: adj_model_sym.span,
                adj_count: fk.targets.len(),
                field_count: fk.fields.len(),
            });
            return;
        }

        let composite_id = if fk.targets.len() > 1 {
            let id = self.composite_seed;
            self.composite_seed += 1;
            Some(id)
        } else {
            None
        };

        for (field, adj_field_sym) in fk.fields.iter().zip(&fk.targets) {
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
        nav: &'p NavigationBlock<'src>,
        table: &SymbolTable<'src, 'p>,
    ) {
        let field = &nav.field.inner;

        let Some(target_block) = table.models.get(nav.model.name) else {
            ma.sink
                .push(SemanticError::UnresolvedSymbol { symbol: &nav.model });
            return;
        };

        let target_backing = self.resolve_target_backing(table, target_block);

        // Each key maps a discriminator on the target to a local field on this model.
        let mut keys = Vec::with_capacity(nav.keys.len());
        for key in &nav.keys {
            let Some(target_field) = table.local.get(&LocalSymbolKind::ModelField {
                model: nav.model.name,
                name: key.target.name,
            }) else {
                ma.sink.push(SemanticError::UnresolvedSymbol {
                    symbol: &key.target,
                });
                continue;
            };

            let Some(local) = key.local.as_ref() else {
                ma.sink.push(SemanticError::RelationMissingLocalKey {
                    target: &key.target,
                });
                continue;
            };
            let Some(local_field) = table.local.get(&LocalSymbolKind::ModelField {
                model: self.name,
                name: local.name,
            }) else {
                ma.sink
                    .push(SemanticError::UnresolvedSymbol { symbol: local });
                continue;
            };

            // A foreign-key local/target column carries no declared type here (it inherits
            // one from its referenced column later), so only compare concretely-typed sides.
            //
            // TODO: An error can slip by if the local or target is an FK, should try to fix.
            let both_resolved = !matches!(local_field.cidl_type, CidlType::Void)
                && !matches!(target_field.cidl_type, CidlType::Void);
            if both_resolved && local_field.cidl_type != target_field.cidl_type {
                ma.sink.push(SemanticError::ArgTypeMismatch {
                    field: &key.target,
                    arg: local,
                });
            }

            keys.push(NavigationKeyMapping {
                local: local.name,
                target: key.target.name,
            });
        }

        // Every route field of the target must be supplied as a key so the target's
        // state can be constructed. Durable Object shard fields are coerced into route fields,
        // so they are also required to be supplied as keys.
        let shard_fields = target_block.shard_args.as_deref().unwrap_or_default();
        let route_fields = target_block.blocks.inners().flat_map(|b| match b {
            ModelBlockKind::Route(symbols) => symbols.as_slice(),
            _ => &[],
        });

        for route in shard_fields.iter().chain(route_fields) {
            if !nav.keys.iter().any(|k| k.target.name == route.name) {
                ma.sink.push(SemanticError::RelationMissingDiscriminator {
                    field,
                    missing: route.name,
                });
            }
        }

        let object = CidlType::Object {
            name: target_block.symbol.name,
        };
        let (cidl_type, cardinality) = match nav.cardinality {
            Cardinality::One => (object, NavigationCardinality::One),
            Cardinality::Many => (
                CidlType::Array(Box::new(object)),
                NavigationCardinality::Many,
            ),
        };

        self.navigation_fields.push(NavigationField {
            field: Field {
                name: field.name.into(),
                cidl_type,
            },
            model_reference: target_block.symbol.name,
            target_backing,
            cardinality,
            keys,
        });
    }

    /// Resolves the [ModelBacking] of a navigation target from its AST, without
    /// requiring the target [Model] to have been built yet. Returns `None` for a
    /// worker-backed (binding-less) target.
    fn resolve_target_backing(
        &self,
        table: &SymbolTable<'src, 'p>,
        target: &'p ModelBlock<'src>,
    ) -> Option<ModelBacking<'src>> {
        let binding = target.database_binding.as_ref()?;

        if let Some(durable) = table.durable_bindings.get(binding.name) {
            let fields = durable
                .shard_blocks
                .inners()
                .flat_map(|s| &s.fields)
                .map(|f| f.name)
                .collect();
            return Some(ModelBacking {
                binding: binding.name,
                fields,
                kind: BackingKind::DurableObject,
            });
        }

        Some(ModelBacking {
            binding: binding.name,
            fields: Vec::new(),
            kind: BackingKind::D1,
        })
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
            (_, Some(durable)) => durable,
            (None, None) => {
                // The binding is invalid but the error has already been sunk during env validation
                return;
            }
        };

        let (template_args, shard_args): (Vec<_>, Vec<_>) = kv
            .args
            .iter()
            .partition(|arg| templates.iter().any(|t| t.field.name == arg.target.name));

        // Exactly one storage template must be referenced
        let [template_arg] = template_args.as_slice() else {
            ma.sink.push(SemanticError::KvTemplateCount {
                field: &kv.field,
                count: template_args.len(),
            });
            return;
        };

        let Some((template, segments)) = self.resolve_binding_ref(
            ma,
            table,
            templates,
            &kv.binding,
            &template_arg.target,
            &template_arg.local,
            &kv.field,
        ) else {
            return;
        };

        // Any non-template arg must be a shard argument
        let shard_fields =
            self.resolve_kv_shard_args(ma, table, &kv.binding, &shard_args, &kv.field);

        self.kv_fields.push(KvField {
            field: ValidatedField {
                name: kv.field.name.into(),
                cidl_type: template.field.cidl_type.clone(),
                validators: template.field.validators.clone(),
            },
            binding: kv.binding.name,
            segments,
            shard_fields,
        });
    }

    /// Validates the non-template args of a KV reference as shard discriminators.
    ///
    /// For a Durable Object binding, every shard field must be supplied exactly once, each
    /// mapped to a single local field of the same type; the resolved local field names are
    /// returned in shard-declaration order. Any supplied arg whose `target` is not a shard
    /// field of the binding is unresolved — this also rejects every stray arg on a Workers
    /// KV binding, which has no shards.
    fn resolve_kv_shard_args(
        &self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        table: &SymbolTable<'src, 'p>,
        binding: &'p Symbol<'src>,
        shard_args: &[&'p KvFieldArgument<'src>],
        field: &'p Symbol<'src>,
    ) -> Vec<&'src str> {
        let shard_fields: Vec<_> = table
            .durable_bindings
            .get(binding.name)
            .map(|d| d.shard_blocks.inners().flat_map(|s| &s.fields).collect())
            .unwrap_or_default();

        // Every supplied arg must name a real shard field of the binding.
        for arg in shard_args {
            if !shard_fields.iter().any(|s| s.name == arg.target.name) {
                ma.sink.push(SemanticError::UnresolvedSymbol {
                    symbol: &arg.target,
                });
            }
        }

        let mut locals = Vec::new();
        for shard in shard_fields {
            let Some(arg) = shard_args.iter().find(|a| a.target.name == shard.name) else {
                ma.sink.push(SemanticError::RelationMissingDiscriminator {
                    field,
                    missing: shard.name,
                });
                continue;
            };

            // `shardField(local)` supplies exactly one local field for the shard.
            let [local] = arg.local.as_slice() else {
                ma.sink.push(SemanticError::ArgCountMismatch {
                    field: &arg.target,
                    expected: 1,
                    got: arg.local.len(),
                });
                continue;
            };

            let Some(local_field) = table.local.get(&LocalSymbolKind::ModelField {
                model: self.name,
                name: local.name,
            }) else {
                ma.sink
                    .push(SemanticError::UnresolvedSymbol { symbol: local });
                continue;
            };

            // A shard field's local supplier may be a route field (no declared type here),
            // so only compare concretely-typed sides.
            let both_resolved = !matches!(local_field.cidl_type, CidlType::Void)
                && !matches!(shard.cidl_type, CidlType::Void);
            if both_resolved && local_field.cidl_type != shard.cidl_type {
                ma.sink.push(SemanticError::ArgTypeMismatch {
                    field: &arg.target,
                    arg: local,
                });
            }

            locals.push(local.name);
        }

        locals
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

        let Some((template, segments)) = self.resolve_binding_ref(
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
            segments,
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
    fn resolve_binding_ref(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        table: &SymbolTable<'src, 'p>,
        templates: &'sem [BindingTemplate<'src>],
        binding_sym: &'p Symbol<'src>,
        template_sym: &'p Symbol<'src>,
        args: &'p [Symbol<'src>],
        field: &'p Symbol<'src>,
    ) -> Option<(
        &'sem BindingTemplate<'src>,
        Vec<TemplateSegment<'src, &'src str>>,
    )> {
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
        let mut param_to_arg: HashMap<&str, &'src str> = HashMap::new();
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

            // Map the template's `{param}` placeholder to the model's `{arg}` field.
            param_to_arg.insert(param.name.as_ref(), arg.name);
        }

        // Rebuild the key segments with each template param swapped for the
        // model field name that supplies it.
        let segments = wrangler_binding_template
            .segments
            .iter()
            .map(|segment| match segment {
                TemplateSegment::Literal(text) => TemplateSegment::Literal(text.clone()),
                TemplateSegment::Value(param) => TemplateSegment::Value(param_to_arg[param]),
            })
            .collect();

        Some((wrangler_binding_template, segments))
    }
}

/// Kahns algorithm for topological sort + cycle detection.
///
/// If no cycles, returns a map of name to position used for sorting
/// the original collection.
fn kahns<'src, 'p>(
    graph: BTreeMap<&'src str, Vec<&'src str>>,
    mut in_degree: BTreeMap<&'src str, usize>,
    len: usize,
) -> Result<HashMap<&'src str, usize>, SemanticError<'src, 'p>> {
    let mut queue = in_degree
        .iter()
        .filter_map(|(&name, &deg)| (deg == 0).then_some(name))
        .collect::<VecDeque<_>>();

    let mut rank = HashMap::with_capacity(len);
    let mut counter = 0usize;

    while let Some(name) = queue.pop_front() {
        rank.insert(name, counter);
        counter += 1;

        if let Some(adjs) = graph.get(name) {
            for adj in adjs {
                let deg = in_degree.get_mut(adj).expect("names to be validated");
                *deg -= 1;

                if *deg == 0 {
                    queue.push_back(adj);
                }
            }
        }
    }

    if rank.len() != len {
        let cycle: Vec<&str> = in_degree
            .iter()
            .filter_map(|(&n, &d)| (d > 0).then_some(n))
            .collect();

        if !cycle.is_empty() {
            return Err(SemanticError::CyclicalRelationship { cycle });
        }
    }

    Ok(rank)
}
