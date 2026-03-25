use ast::{
    CidlType, D1NavigationProperty, ForeignKey, Model, NavigationPropertyKind, Symbol, SymbolKind,
    SymbolRef, SymbolTable, WranglerEnvBindingKind,
};
use frontend::{ForeignKeyTag, ModelBlock, NavigationTag};
use indexmap::IndexMap;

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    ops::Not,
};

use crate::{
    ensure,
    err::{BatchResult, CompilerErrorKind, ErrorSink},
};

type AdjacencyList = BTreeMap<SymbolRef, Vec<SymbolRef>>;

#[derive(Default)]
pub struct D1ModelAnalysis {
    sink: ErrorSink,
    in_degree: BTreeMap<SymbolRef, usize>,
    graph: BTreeMap<SymbolRef, Vec<SymbolRef>>,

    /// Maps a field foreign key reference to the model it is referencing
    /// Ie, Person.dogId => { (Person, dogId): "Dog" }
    model_field_to_adj_model: HashMap<(SymbolRef, SymbolRef), SymbolRef>,
}
impl D1ModelAnalysis {
    pub fn analyze(
        mut self,
        d1_model_blocks: HashMap<SymbolRef, &ModelBlock>,
        table: &mut SymbolTable,
    ) -> BatchResult<IndexMap<SymbolRef, Model>> {
        let mut models = IndexMap::new();

        for model_block in d1_model_blocks.values() {
            if let Some(model) = self.model(model_block, d1_model_blocks.clone(), table) {
                models.insert(model.symbol, model);
            }
        }

        match kahns(self.graph, self.in_degree, d1_model_blocks.len()) {
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

    /// Validates a D1 model, returning an ast [Model]
    fn model(
        &mut self,
        model_block: &ModelBlock,
        d1_model_blocks: HashMap<SymbolRef, &ModelBlock>,
        table: &mut SymbolTable,
    ) -> Option<Model> {
        let Some(d1_binding) = &model_block.d1_binding else {
            self.sink.push(CompilerErrorKind::D1ModelMissingD1Binding {
                model: model_block.id,
            });
            return None;
        };

        let Some(binding_symbol) = table.lookup(d1_binding.env_binding) else {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: d1_binding.env_binding,
            });
            return None;
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
            return None;
        };

        // At least one primary key must be defined
        if model_block.primary_keys.is_empty() {
            self.sink.push(CompilerErrorKind::D1ModelMissingPrimaryKey {
                model: model_block.id,
            });
            return None;
        }

        // Columns
        let mut columns = HashSet::new();
        let mut primary_key_columns = HashSet::new();
        for field in &model_block.fields {
            if !is_valid_sql_type(&field.cidl_type) {
                continue;
            }

            columns.insert(field.id);

            let is_pk = model_block.primary_keys.iter().any(|id| *id == field.id);
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
                &d1_model_blocks,
            );
            if let Some(fk) = fk_result {
                foreign_keys.push(fk);
            }
        }

        // Navigation properties
        let mut navigation_properties = Vec::new();
        for nav in &model_block.navigation_properties {
            let nav_result = self.nav(model_block, nav, table, &d1_model_blocks);

            if let Some(nav) = nav_result {
                navigation_properties.push(nav);
            }
        }

        return Some(Model {
            hash: 0,
            symbol: model_block.id,
            d1_binding: Some(d1_binding.env_binding),
            columns,
            primary_key_columns,
            foreign_keys,
            navigation_properties,
        });
    }

    /// Validates a foreign key, returning an ast [ForeignKey]
    fn foreign_key(
        &mut self,
        model_block: &ModelBlock,
        fk: &ForeignKeyTag,
        columns: &HashSet<SymbolRef>,
        fk_columns_seen: &mut HashSet<SymbolRef>,
        table: &mut SymbolTable,
        d1_model_blocks: &HashMap<SymbolRef, &ModelBlock>,
    ) -> Option<ForeignKey> {
        if table.lookup(fk.adj_model).is_none() {
            self.sink.push(CompilerErrorKind::UnresolvedSymbol {
                symbol: fk.adj_model,
            });
            return None;
        }

        let Some(adj_model) = d1_model_blocks.get(&fk.adj_model) else {
            self.sink
                .push(CompilerErrorKind::ForeignKeyReferencesNonD1Model {
                    tag: fk.id,
                    model: fk.adj_model,
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
        let is_nullable = table.lookup(first_ref).and_then(|sym| match &sym.kind {
            SymbolKind::ModelField { cidl_type, .. } => Some(cidl_type.is_nullable()),
            _ => None,
        });

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

                let SymbolKind::ModelField {
                    cidl_type: field_cidl_type,
                    ..
                } = &field_sym.kind
                else {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk.id,
                            column: *field,
                        },
                    );
                    continue;
                };

                if let Some(is_nullable) = is_nullable {
                    if field_cidl_type.is_nullable() != is_nullable {
                        self.sink
                            .push(CompilerErrorKind::ForeignKeyInconsistentNullability {
                                tag: fk.id,
                                first_column: first_ref,
                                second_column: *field,
                            });
                    }
                }

                field_cidl_type
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

                if !d1_model_blocks.get(&fk.adj_model).is_some() {
                    self.sink
                        .push(CompilerErrorKind::ForeignKeyReferencesNonD1Model {
                            tag: fk.id,
                            model: fk.adj_model,
                        });
                    continue;
                }

                let SymbolKind::ModelField {
                    parent: adj_field_parent,
                    cidl_type: adj_field_cidl_type,
                } = &adj_field_sym.kind
                else {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk.id,
                            column: *adj_field,
                        },
                    );
                    continue;
                };

                if !is_valid_sql_type(adj_field_cidl_type) {
                    self.sink.push(
                        CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                            tag: fk.id,
                            column: *adj_field,
                        },
                    );
                }

                ensure!(
                    *adj_field_parent == fk.adj_model,
                    self.sink,
                    CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn {
                        tag: fk.id,
                        column: *adj_field,
                    }
                );

                adj_field_cidl_type
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
        table: &mut SymbolTable,
        d1_model_blocks: &HashMap<SymbolRef, &ModelBlock>,
    ) -> Option<D1NavigationProperty> {
        let Some(nav_field_sym) = table.lookup(nav.field) else {
            self.sink
                .push(CompilerErrorKind::UnresolvedSymbol { symbol: nav.id });
            return None;
        };

        let SymbolKind::ModelField {
            parent,
            cidl_type: nav_field_cidl_type,
        } = &nav_field_sym.kind
        else {
            self.sink.push(
                CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                    tag: nav.id,
                    field: nav.field,
                },
            );
            return None;
        };

        // The nav property must exist on this model
        if *parent != model_block.id {
            self.sink.push(
                CompilerErrorKind::NavigationPropertyReferencesInvalidOrUnknownField {
                    tag: nav.id,
                    field: nav.field,
                },
            );
            return None;
        }

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

        let Some(adj_model) = d1_model_blocks.get(&adj_model_sym.id) else {
            self.sink
                .push(CompilerErrorKind::NavigationPropertyReferencesNonD1Model {
                    tag: nav.id,
                    model: adj_model_sym.id,
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

        match unwrap_arr_and_null(nav_field_cidl_type) {
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

        let has_arr = matches!(nav_field_cidl_type, CidlType::Array(_));
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

                D1NavigationProperty {
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

                D1NavigationProperty {
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

                D1NavigationProperty {
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

// Kahns algorithm for topological sort + cycle detection.
// If no cycles, returns a map of id to position used for sorting the original collection.
fn kahns(
    graph: AdjacencyList,
    mut in_degree: BTreeMap<SymbolRef, usize>,
    len: usize,
) -> Result<HashMap<SymbolRef, usize>, CompilerErrorKind> {
    let mut queue = in_degree
        .iter()
        .filter_map(|(&name, &deg)| (deg == 0).then_some(name))
        .collect::<VecDeque<_>>();

    let mut rank = HashMap::with_capacity(len);
    let mut counter = 0usize;

    while let Some(id) = queue.pop_front() {
        rank.insert(id, counter);
        counter += 1;

        if let Some(adjs) = graph.get(&id) {
            for adj in adjs {
                let deg = in_degree.get_mut(adj).expect("names to be validated");
                *deg -= 1;

                if *deg == 0 {
                    queue.push_back(*adj);
                }
            }
        }
    }

    if rank.len() != len {
        let cycle: Vec<SymbolRef> = in_degree
            .iter()
            .filter_map(|(&n, &d)| (d > 0).then_some(n))
            .collect();

        if cycle.len() > 0 {
            return Err(CompilerErrorKind::CyclicalModelRelationship { cycle });
        }
    }

    Ok(rank)
}
