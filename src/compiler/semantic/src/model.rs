use crate::{
    LocalSymbolKind, SymbolTable, ensure,
    err::{BatchResult, ErrorSink, SemanticError},
    is_valid_sql_type, kahns, resolve_validator_tags,
};
use frontend::{
    ForeignBlock, KvFieldBlock, ModelBlock, ModelBlockKind, R2FieldBlock, SpdSlice, SqlBlockKind,
    Symbol, Tag,
};
use idl::{
    Binding, BindingTemplate, CidlType, Column, CrudKind, Field, ForeignKeyReference, KvField,
    Model, NavigationField, NavigationFieldKind, R2Field, ValidatedField, WranglerEnv,
};
use indexmap::IndexMap;
use std::collections::BTreeMap;
use std::collections::HashSet;

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

            // Validate CRUD operations
            for crud in &cruds {
                // List requires a D1 binding
                ensure!(
                    !matches!(crud.inner, CrudKind::List) || model.backing_binding.is_some(),
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

    has_defined_pk: bool,
    unique_seed: usize,
    composite_seed: usize,
    primary_columns: Vec<Column<'src>>,
    columns: Vec<Column<'src>>,
    navigation_fields: Vec<NavigationField<'src>>,
    kv_fields: Vec<KvField<'src>>,
    r2_fields: Vec<R2Field<'src>>,
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

        // Models with SQL columns require a D1 binding
        let has_sql_blocks = self.model.blocks.inners().any(|b| {
            matches!(
                b,
                ModelBlockKind::Column(_)
                    | ModelBlockKind::Foreign(_)
                    | ModelBlockKind::Primary(_)
                    | ModelBlockKind::Unique(_)
                    | ModelBlockKind::Navigation(_)
            )
        });

        let binding = if has_sql_blocks || self.model.backing_binding.is_some() {
            let Some(binding_sym) = self.model.backing_binding.as_ref() else {
                ma.sink
                    .push(SemanticError::D1ModelMissingD1Binding { model: self.symbol });
                return None;
            };

            let is_valid_d1 = table
                .d1_bindings
                .iter()
                .flat_map(|b| b.bindings.iter())
                .any(|s| s.name == binding_sym.name);
            if !is_valid_d1 {
                ma.sink.push(SemanticError::D1ModelInvalidD1Binding {
                    model: self.symbol,
                    binding: binding_sym,
                });
                return None;
            };

            Some(binding_sym)
        } else {
            None
        };

        for block in self.model.blocks.inners() {
            match block {
                ModelBlockKind::Column(symbols) => {
                    for symbol in symbols {
                        self.column(ma, symbol, false);
                    }
                }
                ModelBlockKind::Foreign(fk) => {
                    let binding_name = binding.unwrap().name;
                    self.foreign(ma, table, binding_name, fk, false);
                }
                ModelBlockKind::Primary(blocks) => {
                    let binding_name = binding.unwrap().name;

                    for block in blocks {
                        match &block.inner {
                            SqlBlockKind::Column(symbol) => {
                                self.column(ma, symbol, true);
                            }
                            SqlBlockKind::Foreign(foreign_block) => {
                                self.foreign(ma, table, binding_name, foreign_block, true)
                            }
                        }
                    }
                }
                ModelBlockKind::Navigation(navigation_block) => self.nav(
                    ma,
                    binding.unwrap().name,
                    &navigation_block.adj,
                    &navigation_block.nav.inner,
                    false,
                    table,
                ),
                ModelBlockKind::Unique(_) | ModelBlockKind::Kv(_) | ModelBlockKind::R2(_) => {
                    // Processed once all columns are built
                }
            }
        }

        for block in self.model.blocks.inners() {
            match block {
                ModelBlockKind::Unique(fields) => self.unique_constraint(ma, fields),
                ModelBlockKind::Kv(kv) => self.kv_field(ma, table, kv),
                ModelBlockKind::R2(r2) => self.r2_field(ma, table, r2),
                _ => {}
            }
        }

        if binding.is_some() && !self.has_defined_pk {
            ma.sink
                .push(SemanticError::D1ModelMissingPrimaryKey { model: self.symbol });
            return None;
        }

        let binding_name = binding.map(|b| b.name);
        Some(Model {
            name: self.name,
            backing_binding: binding_name,
            primary_columns: self.primary_columns,
            columns: self.columns,
            kv_fields: self.kv_fields,
            r2_fields: self.r2_fields,
            navigation_fields: self.navigation_fields,
            ..Default::default()
        })
    }

    fn column(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        symbol: &'p Symbol<'src>,
        is_primary: bool,
    ) {
        self.has_defined_pk |= is_primary;
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

    fn foreign(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        table: &SymbolTable<'src, 'p>,
        binding: &'src str,
        fk: &'p ForeignBlock<'src>,
        is_primary: bool,
    ) {
        self.has_defined_pk |= is_primary;

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
        let adj_binding = adj_model_block.backing_binding.as_ref();
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

        if let Some(nav_field) = &fk.nav {
            self.nav(ma, binding, &fk.adj, &nav_field.inner.symbol, true, table);
        }
    }

    fn nav(
        &mut self,
        ma: &mut ModelAnalysis<'src, 'p, 'sem>,
        binding: &'src str,
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
        let adj_binding = adj_model_block.backing_binding.as_ref();
        if adj_binding.map(|s| s.name) != Some(binding) {
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
        }
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
        if !table.kv_bindings.contains_key(&kv.binding.name) {
            ma.sink.push(SemanticError::UnresolvedSymbol {
                symbol: &kv.binding,
            });
            return;
        }

        let Some((template, key_format)) = self.resolve_binding_ref(
            ma,
            table,
            ma.env.kv_bindings.as_slice(),
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

        let Some((template, key_format)) = self.resolve_binding_ref(
            ma,
            table,
            &ma.env.r2_bindings,
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

    /// Resolves a KV/R2 binding reference against the wrangler env and validates
    /// its args against the binding template's params.
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
        bindings: &'sem [Binding<'src>],
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

        let Some(wrangler_binding) = bindings.iter().find(|b| b.name == binding_sym.name) else {
            // The binding is invalid but the error has already been sunk during env validation
            return None;
        };

        let Some(wrangler_binding_template) = wrangler_binding
            .templates
            .iter()
            .find(|f| f.field.name == template_sym.name)
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

            let Some(column) = self
                .primary_columns
                .iter_mut()
                .chain(&mut self.columns)
                .find(|f| f.field.name == arg.name)
            else {
                // The referenced symbol exists on the model but is not a column.
                ma.sink.push(SemanticError::ArgTypeMismatch { field, arg });
                return None;
            };

            if column.field.cidl_type != param.cidl_type {
                ma.sink.push(SemanticError::ArgTypeMismatch { field, arg });
                return None;
            }

            // Inherit validators from the binding template param onto the model's column
            column.field.validators.extend(param.validators.clone());

            // Replace the `{param}` placeholder with `{arg}`
            key_format =
                key_format.replace(&format!("{{{}}}", param.name), &format!("{{{}}}", arg.name));
        }

        Some((wrangler_binding_template, key_format))
    }
}
