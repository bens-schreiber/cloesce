use ast::{
    CidlType, CloesceAst, FileSpan, ForeignKey, Model, Symbol, SymbolKind, SymbolRef, WranglerEnv,
    WranglerEnvBindingKind, WranglerSpec,
};
use frontend::{ModelBlock, ParseAst};

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    ops::Not,
};

use crate::err::{BatchResult, CompilerErrorKind, ErrorSink};

pub mod err;

pub struct SymbolTable {
    table: HashMap<SymbolRef, Symbol>,
}

impl SymbolTable {
    /// Converts all declared [ParseId]s into [Symbol]s, catching surface errors along the way
    fn from_parse(parse: &ParseAst, sink: &mut ErrorSink) -> BatchResult<SymbolTable> {
        let mut table = SymbolTable {
            table: HashMap::new(),
        };

        for env in &parse.wrangler_envs {
            let symbol = Symbol {
                id: env.id,
                name: String::default(),
                span: FileSpan {
                    start: env.span.start,
                    end: env.span.end,
                    file: env.file.clone(),
                },
                kind: SymbolKind::WranglerEnvDecl,
            };

            if let Some(existing) = table.table.insert(env.id, symbol) {
                sink.push(CompilerErrorKind::DuplicateSymbol);
                table.table.insert(env.id, existing);
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
                let symbol = Symbol {
                    id: binding.0.id,
                    name: binding.0.name.clone(),
                    span: FileSpan {
                        start: binding.0.span.start,
                        end: binding.0.span.end,
                        file: env.file.clone(),
                    },
                    kind: SymbolKind::WranglerEnvBinding {
                        kind: binding.1.clone(),
                    },
                };

                if let Some(existing) = table.table.insert(binding.0.id, symbol) {
                    sink.push(CompilerErrorKind::DuplicateSymbol);
                    table.table.insert(binding.0.id, existing);
                }
            }

            for var in &env.vars {
                let symbol = Symbol {
                    id: var.id,
                    name: var.name.clone(),
                    span: FileSpan {
                        start: var.span.start,
                        end: var.span.end,
                        file: env.file.clone(),
                    },
                    kind: SymbolKind::WranglerEnvVar {
                        cidl_type: var.cidl_type.clone(),
                    },
                };

                if let Some(existing) = table.table.insert(var.id, symbol) {
                    sink.push(CompilerErrorKind::DuplicateSymbol);
                    table.table.insert(var.id, existing);
                }
            }
        }

        for model in &parse.models {
            let symbol = Symbol {
                id: model.id,
                name: model.name.clone(),
                span: FileSpan {
                    start: model.span.start,
                    end: model.span.end,
                    file: model.file.clone(),
                },
                kind: SymbolKind::ModelDecl,
            };

            if let Some(existing) = table.table.insert(model.id, symbol) {
                sink.push(CompilerErrorKind::DuplicateSymbol);
                table.table.insert(model.id, existing);
            }

            for field in &model.fields {
                let symbol = Symbol {
                    id: field.id,
                    name: field.name.clone(),
                    span: FileSpan {
                        start: field.span.start,
                        end: field.span.end,
                        file: model.file.clone(),
                    },
                    kind: SymbolKind::ModelField {
                        parent: model.id,
                        cidl_type: field.cidl_type.clone(),
                    },
                };

                if let Some(existing) = table.table.insert(field.id, symbol) {
                    sink.push(CompilerErrorKind::DuplicateSymbol);
                    table.table.insert(field.id, existing);
                }
            }
        }

        // TODO: rest

        Ok(table)
    }

    fn lookup(&self, id: usize) -> Option<&Symbol> {
        self.table.get(&id)
    }
}

type AdjacencyList<'a> = BTreeMap<&'a str, Vec<&'a str>>;

pub struct SemanticAnalysis;
impl SemanticAnalysis {
    pub fn analyze(parse: ParseAst, spec: &WranglerSpec) -> BatchResult<()> {
        let mut sink = ErrorSink::new();
        let mut ast = CloesceAst::default();

        let mut table = SymbolTable::from_parse(&parse, &mut sink)?;

        let wrangler_env = Self::wrangler(&parse, &mut table, spec, &mut sink)?;

        let models = Self::models(&parse, &mut table, &mut sink);

        sink.finish()?;

        Ok(())
    }

    /// Validates:
    /// - At most one WranglerEnv is defined
    /// - If a WranglerEnv is not defined, then no models can be defined
    /// - All bindings are consistent with the Wrangler config
    /// - No duplicate field names are defined in the WranglerEnv
    ///
    /// Returns a WranglerEnv with symbol references to all bindings if a WranglerEnv is defined, otherwise returns None.
    fn wrangler(
        parse: &ParseAst,
        table: &mut SymbolTable,
        spec: &WranglerSpec,
        sink: &mut ErrorSink,
    ) -> BatchResult<Option<WranglerEnv>> {
        ensure_bail!(
            parse.wrangler_envs.len() < 2,
            sink,
            CompilerErrorKind::MultipleWranglerEnvBlocks
        );

        let Some(parsed_env) = parse.wrangler_envs.first() else {
            ensure_bail!(
                parse.models.is_empty(),
                sink,
                CompilerErrorKind::MissingWranglerEnvBlock
            );

            return Ok(None);
        };

        let mut vars = HashSet::new();
        let mut d1_bindings = HashSet::new();
        let mut kv_bindings = HashSet::new();
        let mut r2_bindings = HashSet::new();

        for var in &parsed_env.vars {
            ensure_sink!(
                spec.vars.contains_key(var.name.as_str()),
                sink,
                CompilerErrorKind::WranglerBindingInconsistentWithSpec
            );

            vars.insert(var.id);
        }

        for db in &parsed_env.d1_bindings {
            ensure_sink!(
                spec.d1_databases
                    .iter()
                    .any(|d| d.binding.as_ref().is_some_and(|b| b == db.name.as_str())),
                sink,
                CompilerErrorKind::WranglerBindingInconsistentWithSpec
            );

            d1_bindings.insert(db.id);
        }

        for kv in &parsed_env.kv_bindings {
            ensure_sink!(
                spec.kv_namespaces
                    .iter()
                    .any(|ns| ns.binding.as_ref().is_some_and(|b| b == kv.name.as_str())),
                sink,
                CompilerErrorKind::WranglerBindingInconsistentWithSpec
            );

            kv_bindings.insert(kv.id);
        }

        for r2 in &parsed_env.r2_bindings {
            ensure_sink!(
                spec.r2_buckets
                    .iter()
                    .any(|b| b.binding.as_ref().is_some_and(|b| b == r2.name.as_str())),
                sink,
                CompilerErrorKind::WranglerBindingInconsistentWithSpec
            );

            r2_bindings.insert(r2.id);
        }

        Ok(Some(WranglerEnv {
            symbol: parsed_env.id,
            d1_bindings: d1_bindings,
            kv_bindings: kv_bindings,
            r2_bindings: r2_bindings,
            vars: vars,
        }))
    }

    fn models(
        parse: &ParseAst,
        table: &mut SymbolTable,
        sink: &mut ErrorSink,
    ) -> BatchResult<HashMap<SymbolRef, Model>> {
        let mut models = HashMap::new();

        let mut d1_model_blocks = HashMap::<SymbolRef, &ModelBlock>::new();
        for model in &parse.models {
            if model.d1_binding.is_some()
                || model.primary_keys.len() > 0
                || model.fields.len() > 0
                || model.navigation_properties.len() > 0
                || model.foreign_keys.len() > 0
            {
                d1_model_blocks.insert(model.id, model);
            }

            if model.kvs.len() > 0 || model.r2s.len() > 0 {
                // Self::kv_r2_models(parse, model, sink);
            }
        }

        if !d1_model_blocks.is_empty() {
            Self::d1_models(&mut models, d1_model_blocks, table, sink)?;
            // d1_models.sort_by_key(|m| rank.get(m.name.as_str()).unwrap_or(&usize::MAX));
        }

        Ok(models)
    }

    fn d1_models(
        models: &mut HashMap<SymbolRef, Model>,
        d1_model_blocks: HashMap<SymbolRef, &ModelBlock>,
        table: &mut SymbolTable,
        sink: &mut ErrorSink,
    ) -> BatchResult<()> {
        // Topo sort and cycle detection
        let mut in_degree = BTreeMap::<SymbolRef, usize>::new();
        let mut graph = BTreeMap::<SymbolRef, Vec<SymbolRef>>::new();

        // Maps a field foreign key reference to the model it is referencing
        // Ie, Person.dogId => { (Person, dogId): "Dog" }
        let mut model_field_to_adj_model = HashMap::<(SymbolRef, SymbolRef), SymbolRef>::new();
        // let mut unvalidated_navs = Vec::new();

        // Maps a m2m unique id to the models that reference the id
        // let mut m2m = HashMap::<String, Vec<&String>>::new();

        for model_block in d1_model_blocks.values() {
            let Some(d1_binding) = &model_block.d1_binding else {
                sink.push(CompilerErrorKind::D1ModelMissingD1Binding);
                continue;
            };

            let Some(binding_symbol) = table.lookup(*d1_binding) else {
                sink.push(CompilerErrorKind::UnresolvedSymbol);
                continue;
            };

            if matches!(
                binding_symbol.kind,
                SymbolKind::WranglerEnvBinding {
                    kind: WranglerEnvBindingKind::D1
                }
            )
            .not()
            {
                sink.push(CompilerErrorKind::D1ModelInvalidD1Binding);
                continue;
            }

            // At least one primary key must be defined
            if model_block.primary_keys.is_empty() {
                sink.push(CompilerErrorKind::D1ModelMissingPrimaryKey);
                continue;
            }

            // Validate columns + intern all fields
            let mut columns = HashSet::new();
            let mut primary_key_columns = HashSet::new();
            for field in &model_block.fields {
                if !is_valid_sql_type(&field.cidl_type) {
                    // Type cannot be a SQL column
                    continue;
                }

                columns.insert(field.id);

                let is_pk = model_block.primary_keys.iter().any(|id| *id == field.id);
                if is_pk {
                    ensure_sink!(
                        !field.cidl_type.is_nullable(),
                        sink,
                        CompilerErrorKind::NullablePrimaryKey
                    );

                    primary_key_columns.insert(field.id);
                }
            }

            graph.entry(model_block.id).or_default();
            in_degree.entry(model_block.id).or_insert(0);

            // Validate foreign keys
            let mut foreign_keys = Vec::new();
            let mut fk_columns_seen = HashSet::<SymbolRef>::new();
            for fk in &model_block.foreign_keys {
                if table.lookup(fk.adj_model).is_none() {
                    sink.push(CompilerErrorKind::UnresolvedSymbol);
                    continue;
                };

                let Some(adj_model) = d1_model_blocks.get(&fk.adj_model) else {
                    sink.push(CompilerErrorKind::ForeignKeyReferencesNonD1Model);
                    continue;
                };

                if fk.adj_model == model_block.id {
                    sink.push(CompilerErrorKind::ForeignKeyReferenceSelf);
                    continue;
                }

                // Must belong to the same database
                if model_block.d1_binding != adj_model.d1_binding {
                    sink.push(CompilerErrorKind::ForeignKeyReferencesDifferentDatabase);
                    continue;
                }

                let is_nullable = {
                    let Some(first_field) = table.lookup(fk.references.first().unwrap().0) else {
                        sink.push(CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn);
                        continue;
                    };

                    let SymbolKind::ModelField { parent, cidl_type } = &first_field.kind else {
                        sink.push(CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn);
                        continue;
                    };

                    cidl_type.is_nullable()
                };

                let mut fk_columns = Vec::new();
                for (field, adj_field) in &fk.references {
                    let Some(field_symbol) = table.lookup(*field) else {
                        sink.push(CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn);
                        continue;
                    };

                    let SymbolKind::ModelField { cidl_type, .. } = &field_symbol.kind else {
                        sink.push(CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn);
                        continue;
                    };

                    if !fk_columns_seen.insert(field_symbol.id) {
                        sink.push(CompilerErrorKind::ForeignKeyColumnAlreadyInForeignKey);
                        continue;
                    }

                    fk_columns.push(field_symbol.id);

                    let Some(adj_field) = adj_model.fields.iter().find(|f| f.id == *adj_field)
                    else {
                        sink.push(CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn);
                        continue;
                    };

                    // Nullability must be consistent between all FK columns
                    if cidl_type.is_nullable() != is_nullable {
                        sink.push(CompilerErrorKind::ForeignKeyInconsistentNullability);
                        continue;
                    }

                    // All columns in a foreign key must be valid SQL types
                    if !is_valid_sql_type(&cidl_type) {
                        sink.push(CompilerErrorKind::InvalidColumnType);
                        continue;
                    }

                    // Types must be equal (comparing root types to allow nullable FKs)
                    if cidl_type.root_type() != adj_field.cidl_type.root_type() {
                        sink.push(CompilerErrorKind::ForeignKeyReferencesIncompatibleColumnType);
                        continue;
                    }

                    model_field_to_adj_model
                        .insert((field_symbol.id, model_block.id), adj_model.id);

                    if !cidl_type.is_nullable() {
                        // One To One: Person has a Dog ..(sql)=> Person has a fk to Dog
                        // Dog must come before Person
                        graph.entry(fk.adj_model).or_default().push(model_block.id);
                        *in_degree.entry(model_block.id).or_insert(0) += 1;
                    }
                }

                foreign_keys.push(ForeignKey {
                    adj_model: fk.adj_model,
                    columns: fk_columns,
                });
            }

            // Validate navigation properties
            let mut navigation_properties = Vec::new();
            for nav in &model_block.navigation_properties {
                let Some(nav_field_sym) = table.lookup(nav.field) else {
                    sink.push(CompilerErrorKind::UnresolvedSymbol);
                    continue;
                };

                let SymbolKind::ModelField { parent, cidl_type } = &nav_field_sym.kind else {
                    sink.push(
                        CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownColumn,
                    );
                    continue;
                };

                // A nav property must exist on this model
                if model_block.id != *parent {
                    sink.push(
                        CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownColumn,
                    );
                    continue;
                }

                let Some(adj_model_sym) = table.lookup(nav.adj_model) else {
                    sink.push(CompilerErrorKind::UnresolvedSymbol);
                    continue;
                };

                if adj_model_sym.id == model_block.id {
                    sink.push(CompilerErrorKind::NavigationPropertyReferencesSelf);
                    continue;
                }

                let Some(adj_model) = d1_model_blocks.get(&adj_model_sym.id) else {
                    sink.push(CompilerErrorKind::ForeignKeyReferencesNonD1Model);
                    continue;
                };

                let referenced_fields = nav
                    .fields
                    .iter()
                    .filter_map(|f| {
                        let Some(field_sym) = table.lookup(*f) else {
                            sink.push(
                                CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownColumn,
                            );
                            return None;
                        };

                        if !columns.contains(&field_sym.id) {
                            sink.push(
                                CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownColumn,
                            );
                            return None;
                        }

                        Some(field_sym)
                    })
                    .collect::<Vec<&Symbol>>();
                if referenced_fields.len() != nav.fields.len() {
                    // Some referenced fields were invalid, errors caught above
                    continue;
                }

                // Ensure both models belong to the same database
                if model_block.d1_binding != adj_model.d1_binding {
                    sink.push(CompilerErrorKind::ForeignKeyReferencesDifferentDatabase);
                    continue;
                }

                // // A nav field must be of cidl type Object, that Object must be the adjacent model OR an array of the adjacent model
                // let name = match &nav_field.cidl_type {
                //     CidlType::Object(name) => {
                //         // Must be a model name
                //     }
                //     _ => {
                //         sink.push(
                //             CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownColumn,
                //         );
                //         continue;
                //     }
                // };
            }

            models.insert(
                model_block.id,
                Model {
                    hash: 0,
                    symbol: model_block.id,
                    d1_binding: Some(*d1_binding),
                    columns,
                    primary_key_columns,
                    foreign_keys,
                    navigation_properties,
                },
            );
        }

        Ok(())
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

//     fn d1_models(ast: &CloesceAst, d1_models: Vec<&Model>) -> Result<HashMap<String, usize>> {
//         for model in &d1_models {

//             // Validate navigation props
//             for nav in &model.navigation_properties {
//                 ensure!(
//                     ast.models.contains_key(nav.model_reference.as_str()),
//                     GeneratorErrorKind::InvalidModelReference,
//                     "{} => {}?",
//                     model.name,
//                     nav.model_reference
//                 );

//                 // Ensure both models belong to the same database
//                 let nav_model = ast.models.get(nav.model_reference.as_str()).unwrap();
//                 ensure!(
//                     model.d1_binding == nav_model.d1_binding,
//                     GeneratorErrorKind::InvalidModelReference,
//                     "{}.{} references {}, but they belong to different databases ({:?} != {:?})",
//                     model.name,
//                     nav.field_name,
//                     nav.model_reference,
//                     model.d1_binding,
//                     nav_model.d1_binding
//                 );

//                 match &nav.kind {
//                     NavigationPropertyKind::OneToOne { key_columns } => {
//                         // nav_model is already retrieved and validated above
//                         ensure!(
//                             key_columns.len() == nav_model.primary_key_columns.len(),
//                             GeneratorErrorKind::InvalidNavigationPropertyReference,
//                             "{}.{} references {} but the number of key columns does not match the number of primary key columns on the model",
//                             model.name,
//                             nav.field_name,
//                             nav.model_reference
//                         );

//                         // Ensure no duplicate key columns
//                         let unique_key_cols: HashSet<&str> =
//                             key_columns.iter().map(|s| s.as_str()).collect();
//                         ensure!(
//                             unique_key_cols.len() == key_columns.len(),
//                             GeneratorErrorKind::InvalidNavigationPropertyReference,
//                             "{}.{} references {} but key columns contain duplicates",
//                             model.name,
//                             nav.field_name,
//                             nav.model_reference
//                         );

//                         let mut referenced_nav_pks = HashSet::new();
//                         for key_ref in key_columns {
//                             let found = model
//                                 .all_columns()
//                                 .map(|(c, _)| c)
//                                 .filter(|c| c.value.name == *key_ref)
//                                 .find(|c| {
//                                     c.foreign_key_reference
//                                         .as_ref()
//                                         .map(|fk| fk.model_name.as_str())
//                                         == Some(nav.model_reference.as_str())
//                                 });

//                             let Some(col) = found else {
//                                 fail!(
//                                     GeneratorErrorKind::InvalidNavigationPropertyReference,
//                                     "{}.{} references {}.{} which does not exist or is not a foreign key to {}",
//                                     model.name,
//                                     nav.field_name,
//                                     nav.model_reference,
//                                     key_ref,
//                                     nav.model_reference
//                                 );
//                             };

//                             referenced_nav_pks.insert(
//                                 col.foreign_key_reference
//                                     .as_ref()
//                                     .unwrap()
//                                     .column_name
//                                     .as_str(),
//                             );
//                         }

//                         // Ensure all nav model PK columns are referenced exactly once
//                         let nav_pk_names: HashSet<&str> = nav_model
//                             .primary_key_columns
//                             .iter()
//                             .map(|c| c.value.name.as_str())
//                             .collect();

//                         ensure!(
//                             referenced_nav_pks == nav_pk_names,
//                             GeneratorErrorKind::InvalidNavigationPropertyReference,
//                             "{}.{} references {} but the key columns do not cover all primary key columns of the referenced model",
//                             model.name,
//                             nav.field_name,
//                             nav.model_reference
//                         );
//                     }
//                     NavigationPropertyKind::OneToMany { .. } => {
//                         unvalidated_navs.push((&model.name, &nav.model_reference, nav));
//                     }
//                     NavigationPropertyKind::ManyToMany => {
//                         let id = nav.many_to_many_table_name(&model.name);
//                         m2m.entry(id).or_default().push(&model.name);
//                     }
//                 }
//             }
//         }

//         // Validate 1:M nav props
//         for (model_name, nav_model_reference, nav) in unvalidated_navs {
//             let NavigationPropertyKind::OneToMany { key_columns } = &nav.kind else {
//                 continue;
//             };

//             let model = ast.models.get(model_name).unwrap();
//             ensure!(
//                 key_columns.len() == model.primary_key_columns.len(),
//                 GeneratorErrorKind::InvalidNavigationPropertyReference,
//                 "{}.{} references {} but the number of key columns does not match the number of primary key columns on this model",
//                 model_name,
//                 nav.field_name,
//                 nav_model_reference
//             );

//             // Ensure no duplicate key columns
//             let unique_key_cols: HashSet<&str> = key_columns.iter().map(|s| s.as_str()).collect();
//             ensure!(
//                 unique_key_cols.len() == key_columns.len(),
//                 GeneratorErrorKind::InvalidNavigationPropertyReference,
//                 "{}.{} references {} but key columns contain duplicates",
//                 model_name,
//                 nav.field_name,
//                 nav_model_reference
//             );

//             // Track which nav model PK columns are referenced
//             let mut referenced_nav_pks = HashSet::new();

//             let nav_model = ast.models.get(nav_model_reference.as_str()).unwrap();
//             for key_ref in key_columns {
//                 let found = nav_model
//                     .all_columns()
//                     .filter(|(c, _)| c.value.name == *key_ref)
//                     .find(|(c, _)| {
//                         c.foreign_key_reference
//                             .as_ref()
//                             .map(|fk| fk.model_name.as_str())
//                             == Some(model_name)
//                     });

//                 let Some((col, _)) = found else {
//                     fail!(
//                         GeneratorErrorKind::InvalidNavigationPropertyReference,
//                         "{}.{} references {}.{} which does not exist or is not a foreign key to {}",
//                         model_name,
//                         nav.field_name,
//                         nav_model_reference,
//                         key_ref,
//                         model_name
//                     );
//                 };

//                 // Track which nav PK column is being referenced
//                 if let Some(fk) = &col.foreign_key_reference {
//                     referenced_nav_pks.insert(fk.column_name.as_str());
//                 }
//             }

//             // Ensure all current model PK columns are referenced exactly once
//             let model_pk_names: HashSet<&str> = model
//                 .primary_key_columns
//                 .iter()
//                 .map(|c| c.value.name.as_str())
//                 .collect();

//             ensure!(
//                 referenced_nav_pks == model_pk_names,
//                 GeneratorErrorKind::InvalidNavigationPropertyReference,
//                 "{}.{} references {} but the key columns do not cover all primary key columns of this model",
//                 model_name,
//                 nav.field_name,
//                 nav_model_reference
//             );

//             // One To Many: Person has many Dogs (sql)=> Dog has an fk to  Person
//             // Person must come before Dog in topo order
//             graph
//                 .entry(model_name)
//                 .or_default()
//                 .push(nav_model_reference);
//             *in_degree.entry(nav_model_reference).or_insert(0) += 1;
//         }

//         // Validate M:M
//         for (unique_id, jcts) in m2m {
//             if jcts.len() < 2 {
//                 fail!(
//                     GeneratorErrorKind::MissingManyToManyReference,
//                     "Missing junction table for many to many table {}",
//                     unique_id
//                 );
//             }

//             if jcts.len() > 2 {
//                 let joined = jcts
//                     .iter()
//                     .map(|s| s.as_str())
//                     .collect::<Vec<_>>()
//                     .join(",");
//                 fail!(
//                     GeneratorErrorKind::ExtraneousManyToManyReferences,
//                     "Many To Many Table {unique_id} {joined}",
//                 );
//             }

//             // Ensure both models in many-to-many relationship belong to the same database
//             let model1 = ast.models.get(jcts[0].as_str()).unwrap();
//             let model2 = ast.models.get(jcts[1].as_str()).unwrap();
//             ensure!(
//                 model1.d1_binding == model2.d1_binding,
//                 GeneratorErrorKind::InvalidModelReference,
//                 "Many-to-many relationship between {} and {} requires both models to belong to the same database ({:?} != {:?})",
//                 jcts[0],
//                 jcts[1],
//                 model1.d1_binding,
//                 model2.d1_binding
//             );
//         }

//         kahns(graph, in_degree, d1_models.len())
//     }

//     fn kv_r2_models(ast: &CloesceAst, model: &Model) -> Result<()> {
//         // Validate KV key format
//         for kv in &model.kv_objects {
//             // Namespace must exist
//             ensure!(
//                 ast.wrangler_env
//                     .as_ref()
//                     .unwrap()
//                     .kv_bindings
//                     .iter()
//                     .any(|ns| ns == &kv.namespace_binding),
//                 GeneratorErrorKind::InconsistentWranglerBinding,
//                 "{}.{} => {}? No matching KV namespace binding found in WranglerEnv",
//                 model.name,
//                 kv.value.name,
//                 kv.namespace_binding
//             );

//             let vars = extract_braced(&kv.format)?;

//             for var in vars {
//                 ensure!(
//                     model.all_columns().any(|(col, _)| col.value.name == var)
//                         || model.key_params.contains(&var),
//                     GeneratorErrorKind::UnknownKeyReference,
//                     "{}.{} => {} missing key param for variable {}",
//                     model.name,
//                     kv.value.name,
//                     kv.format,
//                     var
//                 )
//             }

//             // Validate value type
//             match kv.value.cidl_type.root_type() {
//                 CidlType::Object(o) | CidlType::Partial(o) => {
//                     ensure!(
//                         is_valid_object_ref(ast, o),
//                         GeneratorErrorKind::UnknownObject,
//                         "{}.{} => {}?",
//                         model.name,
//                         kv.value.name,
//                         o
//                     );
//                 }
//                 CidlType::Inject(o) => {
//                     fail!(
//                         GeneratorErrorKind::UnexpectedInject,
//                         "{}.{} => {}?",
//                         model.name,
//                         kv.value.name,
//                         o
//                     )
//                 }
//                 CidlType::DataSource(reference) => ensure!(
//                     is_valid_data_source_ref(ast, reference),
//                     GeneratorErrorKind::InvalidModelReference,
//                     "{}.{} => {}?",
//                     model.name,
//                     kv.value.name,
//                     reference
//                 ),
//                 _ => {}
//             }
//         }

//         // Validate R2 Key format
//         for r2 in &model.r2_objects {
//             // Bucket binding must exist
//             ensure!(
//                 ast.wrangler_env
//                     .as_ref()
//                     .unwrap()
//                     .r2_bindings
//                     .iter()
//                     .any(|b| b == &r2.bucket_binding),
//                 GeneratorErrorKind::InconsistentWranglerBinding,
//                 "{}.{} => {}? No matching R2 bucket binding found in WranglerEnv",
//                 model.name,
//                 r2.var_name,
//                 r2.bucket_binding
//             );

//             let vars = extract_braced(&r2.format)?;

//             for var in vars {
//                 ensure!(
//                     model.all_columns().any(|(col, _)| col.value.name == var)
//                         || model.key_params.contains(&var),
//                     GeneratorErrorKind::UnknownKeyReference,
//                     "{}.{} => {} missing key param for variable {}",
//                     model.name,
//                     r2.var_name,
//                     r2.format,
//                     var
//                 )
//             }
//         }

//         Ok(())
//     }

//     fn services(ast: &mut CloesceAst) -> Result<()> {
//         // Topo sort and cycle detection
//         let mut in_degree = BTreeMap::<&str, usize>::new();
//         let mut graph = BTreeMap::<&str, Vec<&str>>::new();

//         for (service_name, service) in &ast.services {
//             graph.entry(&service.name).or_default();
//             in_degree.entry(&service.name).or_insert(0);

//             // Validate record
//             ensure!(
//                 *service_name == service.name,
//                 GeneratorErrorKind::InvalidMapping,
//                 "Method record key did not match it's method name? {}: {}",
//                 service_name,
//                 service.name
//             );

//             // Assemble graph
//             for attr in &service.attributes {
//                 if !ast.services.contains_key(&attr.inject_reference) {
//                     continue;
//                 }

//                 graph
//                     .entry(attr.inject_reference.as_str())
//                     .or_default()
//                     .push(&service.name);
//                 in_degree.entry(&service.name).and_modify(|d| *d += 1);
//             }

//             // Validate methods
//             for (method_name, method) in &service.methods {
//                 validate_methods(service_name, method_name, method, ast)?;
//             }
//         }

//         // Sort
//         let rank = kahns(graph, in_degree, ast.services.len())?;
//         ast.services
//             .sort_by_key(|k, _| rank.get(k.as_str()).unwrap());

//         Ok(())
//     }
// }

// /// Extracts braced variables from a format string.
// /// e.g, "users/{userId}/posts/{postId}" => ["userId", "postId"].
// ///
// /// Returns a [GeneratorErrorKind] if the format string is invalid.
// fn extract_braced(s: &str) -> Result<Vec<String>> {
//     let mut out = Vec::new();
//     let mut current = None;

//     for c in s.chars() {
//         match (current.as_mut(), c) {
//             (None, '{') => current = Some(String::new()),
//             (Some(_), '{') => {
//                 fail!(GeneratorErrorKind::InvalidKeyFormat, "nested brace in key");
//             }
//             (Some(buf), '}') => {
//                 out.push(std::mem::take(buf));
//                 current = None;
//             }
//             (Some(buf), c) => buf.push(c),
//             _ => {}
//         }
//     }

//     if current.is_some() {
//         fail!(
//             GeneratorErrorKind::InvalidKeyFormat,
//             "unclosed brace in key"
//         );
//     }

//     Ok(out)
// }

// fn is_valid_object_ref(ast: &CloesceAst, o: &String) -> bool {
//     ast.models.contains_key(o) || ast.poos.contains_key(o)
// }

// fn is_valid_data_source_ref(ast: &CloesceAst, o: &String) -> bool {
//     ast.models.contains_key(o)
// }

// /// Validates an [ApiMethod]'s grammar.
// ///
// /// Returns a [GeneratorErrorKind] on failure.
// fn validate_methods(
//     namespace: &str,
//     method_name: &str,
//     method: &ApiMethod,
//     ast: &CloesceAst,
// ) -> Result<()> {
//     // Validate record
//     ensure!(
//         *method_name == method.name,
//         GeneratorErrorKind::InvalidMapping,
//         "Method record key did not match it's method name? {}: {}",
//         method_name,
//         method.name
//     );

//     // Validate data source reference
//     if let Some(ds) = &method.data_source {
//         ensure!(
//             !method.is_static,
//             GeneratorErrorKind::InvalidDataSourceReference,
//             "{}.{} has a data source but is a static method.",
//             namespace,
//             method.name
//         );

//         let Some(model) = ast.models.get(namespace) else {
//             fail!(
//                 GeneratorErrorKind::InvalidModelReference,
//                 "{}.{} references a data source on an unknown model {}",
//                 namespace,
//                 method.name,
//                 namespace
//             );
//         };

//         ensure!(
//             model.data_sources.contains_key(ds),
//             GeneratorErrorKind::UnknownDataSourceReference,
//             "{}.{} references an unknown data source {} on model {}",
//             namespace,
//             method.name,
//             ds,
//             namespace
//         );
//     }

//     // Validate return type
//     match &method.return_type.root_type() {
//         CidlType::Object(o) | CidlType::Partial(o) => {
//             ensure!(
//                 is_valid_object_ref(ast, o),
//                 GeneratorErrorKind::UnknownObject,
//                 "{}.{}",
//                 namespace,
//                 method.name
//             );
//         }

//         CidlType::DataSource(model_name) => ensure!(
//             is_valid_data_source_ref(ast, model_name),
//             GeneratorErrorKind::UnknownDataSourceReference,
//             "{}.{}",
//             namespace,
//             method.name,
//         ),

//         CidlType::Inject(o) => fail!(
//             GeneratorErrorKind::UnexpectedInject,
//             "{}.{} => {}?",
//             namespace,
//             method.name,
//             o
//         ),
//         CidlType::Stream => ensure!(
//             // Stream or HttpResult<Stream>
//             matches!(method.return_type, CidlType::Stream)
//                 || matches!(&method.return_type, CidlType::HttpResult(boxed) if matches!(**boxed, CidlType::Stream)),
//             GeneratorErrorKind::InvalidStream,
//             "{}.{}",
//             namespace,
//             method.name
//         ),
//         _ => {}
//     }

//     // Validate method params
//     for param in &method.parameters {
//         if let CidlType::DataSource(model_name) = &param.cidl_type {
//             ensure!(
//                 is_valid_data_source_ref(ast, model_name),
//                 GeneratorErrorKind::InvalidModelReference,
//                 "{}.{} data source references {}",
//                 namespace,
//                 method.name,
//                 model_name
//             );

//             continue;
//         }

//         ensure!(
//             !cidl_type_contains!(&param.cidl_type, CidlType::HttpResult(_)),
//             GeneratorErrorKind::NotYetSupported,
//             "Requests currently do not support HttpResult parameters {}.{}.{}",
//             namespace,
//             method.name,
//             param.name
//         );

//         // todo: remove this limitation
//         ensure!(
//             method.http_verb != HttpVerb::Get
//                 || !cidl_type_contains!(&param.cidl_type, CidlType::KvObject(_)),
//             GeneratorErrorKind::NotYetSupported,
//             "GET Requests currently do not support KV Object parameters {}.{}.{}",
//             namespace,
//             method.name,
//             param.name
//         );

//         let root_type = param.cidl_type.root_type();

//         match root_type {
//             CidlType::Void => {
//                 fail!(
//                     GeneratorErrorKind::UnexpectedVoid,
//                     "{}.{}.{}",
//                     namespace,
//                     method.name,
//                     param.name
//                 )
//             }
//             CidlType::Object(o) | CidlType::Partial(o) => {
//                 ensure!(
//                     is_valid_object_ref(ast, o),
//                     GeneratorErrorKind::UnknownObject,
//                     "{}.{}.{}",
//                     namespace,
//                     method.name,
//                     param.name
//                 );

//                 // TODO: remove this
//                 if method.http_verb == HttpVerb::Get {
//                     fail!(
//                         GeneratorErrorKind::NotYetSupported,
//                         "GET Requests currently do not support object parameters {}.{}.{}",
//                         namespace,
//                         method.name,
//                         param.name
//                     )
//                 }
//             }
//             CidlType::R2Object => {
//                 // TODO: remove this
//                 if method.http_verb == HttpVerb::Get {
//                     fail!(
//                         GeneratorErrorKind::NotYetSupported,
//                         "GET Requests currently do not support R2Object parameters {}.{}.{}",
//                         namespace,
//                         method.name,
//                         param.name
//                     )
//                 }
//             }
//             CidlType::DataSource(model_name) => {
//                 ensure!(
//                     ast.models.contains_key(model_name),
//                     GeneratorErrorKind::InvalidModelReference,
//                     "{}.{} data source references {}",
//                     namespace,
//                     method.name,
//                     model_name
//                 )
//             }
//             CidlType::Stream => {
//                 let required_params = method
//                     .parameters
//                     .iter()
//                     .filter(|p| {
//                         !matches!(p.cidl_type, CidlType::Inject(_) | CidlType::DataSource(_))
//                     })
//                     .count();

//                 ensure!(
//                     required_params == 1 && matches!(param.cidl_type, CidlType::Stream),
//                     GeneratorErrorKind::InvalidStream,
//                     "{}.{}",
//                     namespace,
//                     method.name
//                 )
//             }
//             _ => {
//                 // Ignore
//             }
//         }
//     }

//     Ok(())
// }

// // Kahns algorithm for topological sort + cycle detection.
// // If no cycles, returns a map of id to position used for sorting the original collection.
// fn kahns<'a>(
//     graph: AdjacencyList<'a>,
//     mut in_degree: BTreeMap<&'a str, usize>,
//     len: usize,
// ) -> Result<HashMap<String, usize>> {
//     let mut queue = in_degree
//         .iter()
//         .filter_map(|(&name, &deg)| (deg == 0).then_some(name))
//         .collect::<VecDeque<_>>();

//     let mut rank = HashMap::with_capacity(len);
//     let mut counter = 0usize;

//     while let Some(model_name) = queue.pop_front() {
//         rank.insert(model_name.to_string(), counter);
//         counter += 1;

//         if let Some(adjs) = graph.get(model_name) {
//             for adj in adjs {
//                 let deg = in_degree.get_mut(adj).expect("names to be validated");
//                 *deg -= 1;

//                 if *deg == 0 {
//                     queue.push_back(adj);
//                 }
//             }
//         }
//     }

//     if rank.len() != len {
//         let cyclic: Vec<&str> = in_degree
//             .iter()
//             .filter_map(|(&n, &d)| (d > 0).then_some(n))
//             .collect();
//         fail!(
//             GeneratorErrorKind::CyclicalDependency,
//             "{}",
//             cyclic.join(", ")
//         );
//     }

//     Ok(rank)
// }

// /// Ensures that a reference within an include tree exists within the given model.
// ///
// /// Returns the referenced model name if the reference is a navigation property,
// /// or None if the reference is a KV or R2 object.
// fn valid_include_tree_reference(model: &Model, var_name: String) -> Result<Option<&str>> {
//     if let Some(nav) = model
//         .navigation_properties
//         .iter()
//         .find(|nav| nav.field_name == var_name)
//     {
//         return Ok(Some(&nav.model_reference));
//     }

//     if model.kv_objects.iter().any(|kv| kv.value.name == var_name) {
//         return Ok(None);
//     }

//     if model.r2_objects.iter().any(|r2| r2.var_name == var_name) {
//         return Ok(None);
//     }

//     fail!(
//         GeneratorErrorKind::UnknownIncludeTreeReference,
//         "{}.{}",
//         model.name,
//         var_name
//     );
// }
