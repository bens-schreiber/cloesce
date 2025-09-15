use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use common::{CidlForeignKeyKind, CidlType, Model, TypedValue};

use anyhow::{Result, anyhow};
use sea_query::{Alias, ColumnDef, ForeignKey, Index, SqliteQueryBuilder, Table};

fn type_column(column: &mut ColumnDef, ty: &CidlType) -> Result<()> {
    match &ty {
        CidlType::Integer => column.integer(),
        CidlType::Real => column.decimal(),
        CidlType::Text => column.text(),
        CidlType::Blob => column.blob(),
        other => return Err(anyhow!("Invalid SQLite type {:?}", other)),
    };

    Ok(())
}

struct JunctionModel<'a> {
    model_name: &'a str,
    model_pk_name: &'a str,
    model_pk_type: CidlType,
}

#[derive(Default)]
struct JunctionTableBuilder<'a> {
    models: [Option<JunctionModel<'a>>; 2],
}

impl<'a> JunctionTableBuilder<'a> {
    fn model(&mut self, jm: JunctionModel<'a>) -> Result<()> {
        if self.models[0].is_none() {
            self.models[0] = Some(jm);
        } else if self.models[1].is_none() {
            self.models[1] = Some(jm);
        } else {
            return Err(anyhow!(
                "Too many ManyToMany navigation properties for junction table"
            ));
        }

        Ok(())
    }

    fn build(self, unique_id: &String) -> Result<String> {
        let [Some(a), Some(b)] = self.models else {
            return Err(anyhow!("Both models must be set for a junction table"));
        };

        let mut table = Table::create();

        // TODO: Name the junction table in some standard way
        let mut col_a = ColumnDef::new(Alias::new(format!("a_{}", a.model_pk_name)));
        type_column(&mut col_a, &a.model_pk_type)?;

        let mut col_b = ColumnDef::new(Alias::new(format!("b_{}", b.model_pk_name)));
        type_column(&mut col_b, &b.model_pk_type)?;

        table
            .table(Alias::new(unique_id))
            .col(col_a.not_null())
            .col(col_b.not_null())
            .primary_key(
                Index::create()
                    .col(Alias::new(format!("a_{}", a.model_pk_name)))
                    .col(Alias::new(format!("b_{}", b.model_pk_name))),
            )
            .foreign_key(
                ForeignKey::create()
                    .from(
                        Alias::new(unique_id),
                        Alias::new(format!("a_{}", a.model_pk_name)),
                    )
                    .to(Alias::new(a.model_name), Alias::new(a.model_pk_name))
                    .on_update(sea_query::ForeignKeyAction::Cascade)
                    .on_delete(sea_query::ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .from(
                        Alias::new(unique_id),
                        Alias::new(format!("b_{}", b.model_pk_name)),
                    )
                    .to(Alias::new(b.model_name), Alias::new(b.model_pk_name))
                    .on_update(sea_query::ForeignKeyAction::Cascade)
                    .on_delete(sea_query::ForeignKeyAction::Restrict),
            );

        Ok(format!("{};", table.to_string(SqliteQueryBuilder)))
    }
}

/// Validates all models, returning a lookup table of a model name to it's [Model].
///
/// Returns error on
/// - Duplicate Model names
/// - Duplicate column names
/// - Duplicate primary keys
/// - Invalid typed primary keys
/// - Missing primary keys
fn validate_models(models: &[Model]) -> Result<HashMap<&str, &Model>> {
    let mut model_lookup = HashMap::<&str, &Model>::new();
    for model in models {
        // Duplicate models
        if model_lookup.insert(&model.name, model).is_some() {
            return Err(anyhow!("Duplicate model name: {}", model.name));
        }

        let mut column_names = HashSet::new();
        let mut has_pk = false;
        for attr in &model.attributes {
            // Duplicate columns
            if !column_names.insert(attr.value.name.as_str()) {
                return Err(anyhow!(
                    "Duplicate column names {}.{}",
                    model.name,
                    attr.value.name
                ));
            }

            // Validate primary key
            if attr.primary_key {
                if has_pk {
                    return Err(anyhow!("Duplicate primary keys on model {}", model.name));
                }

                if attr.value.nullable {
                    return Err(anyhow!("A primary key cannot be nullable."));
                }

                if attr.foreign_key.is_some() {
                    // TODO: Revisit this, should this design be allowed?
                    return Err(anyhow!("A primary key cannot be a foreign key"));
                }

                has_pk = true;
            }
        }

        if !has_pk {
            return Err(anyhow!("Missing primary key on model {}", model.name));
        }
    }

    Ok(model_lookup)
}

/// Validates foreign key relationships for every [Model] in the CIDL. Returns a
/// SQL-safe topological ordering of the models, along with a vec of all built Many to Many tables
///
/// Returns error on
/// - Unknown foreign key models
/// - Invalid navigation property types
/// - Missing navigation property attributes
/// - Cyclical dependencies
fn validate_fks<'a>(
    models: &[Model],
    model_lookup: &HashMap<&str, &'a Model>,
) -> Result<(Vec<&'a Model>, Vec<String>)> {
    let mut in_degree = BTreeMap::<&str, usize>::new();
    let mut graph = BTreeMap::<&str, Vec<&str>>::new();

    // Maps a model name and a foreign key reference to the model it is referencing
    // Ie, Person.dogId => { (Person, dogId): "Dog" }
    let mut model_reference_to_fk_model = HashMap::new();

    let mut unresolved_nav_props = Vec::new();
    let mut many_to_many = HashMap::<&String, JunctionTableBuilder>::new();
    for model in models {
        graph.entry(&model.name).or_default();
        in_degree.entry(&model.name).or_insert(0);

        let mut pk: Option<&TypedValue> = None;
        for attr in &model.attributes {
            let Some(fk_model) = &attr.foreign_key else {
                if attr.primary_key {
                    pk = Some(&attr.value);
                }
                continue;
            };

            // Validate FK Model
            if !model_lookup.contains_key(fk_model.as_str()) {
                return Err(anyhow!(
                    "Unknown Model for foreign key {}.{} => {}?",
                    model.name,
                    attr.value.name,
                    fk_model
                ));
            }

            model_reference_to_fk_model.insert((&model.name, attr.value.name.as_str()), fk_model);

            if attr.value.nullable {
                // Nullable FK's do not constrain table creation order, and thus
                // can be left out of the topo sort
                continue;
            }

            // One To One: Person has a Dog ..(sql)=> Person has a fk to Dog
            // Dog must come before Person
            graph.entry(fk_model).or_default().push(&model.name);
            in_degree.entry(&model.name).and_modify(|d| *d += 1);
        }

        for nav in &model.navigation_properties {
            match &nav.foreign_key {
                CidlForeignKeyKind::OneToOne { reference } => {
                    // Validate nav prop has a Model type
                    let CidlType::Model(nav_model) = &nav.value.cidl_type else {
                        return Err(anyhow!(
                            "Expected Model type for navigation property {}.{}",
                            model.name,
                            nav.value.name
                        ));
                    };

                    // Validate the nav prop's model exists
                    if !model_lookup.contains_key(nav_model.as_str()) {
                        return Err(anyhow!(
                            "Unknown Model for navigation property {}.{} => {}?",
                            model.name,
                            nav.value.name,
                            nav_model
                        ));
                    }

                    // Validate the nav prop's reference is to a valid fk attribute
                    // in this model
                    if let Some(&fk_model) =
                        model_reference_to_fk_model.get(&(&model.name, reference))
                    {
                        if fk_model != nav_model {
                            return Err(anyhow!(
                                "Mismatched types between foreign key and One to One navigation property ({}.{}) ({})",
                                model.name,
                                nav.value.name,
                                fk_model
                            ));
                        }
                    } else {
                        return Err(anyhow!(
                            "Navigation property {}.{} references {}.{} which does not exist.",
                            model.name,
                            nav.value.name,
                            nav_model,
                            reference
                        ));
                    }

                    // TODO: Revisit this. Should a user be able to decorate a One To One
                    // navigation property, but have no foreign key for it?
                    // ( ie, make the enum OneToOne(Option<String>) )
                }
                CidlForeignKeyKind::OneToMany { reference: _ } => {
                    // Validate FK Type
                    let Some(CidlType::Model(nav_model)) = nav.value.cidl_type.unwrap_array()
                    else {
                        return Err(anyhow!(
                            "Expected collection of Model type for navigation property {}.{}",
                            model.name,
                            nav.value.name
                        ));
                    };

                    if nav.value.nullable {
                        return Err(anyhow!(
                            "One To Many navigation property cannot be nullable {}.{}",
                            model.name,
                            nav.value.name
                        ));
                    }

                    // Validate FK Model
                    if !model_lookup.contains_key(nav_model.as_str()) {
                        return Err(anyhow!(
                            "Unknown Model for navigation property {}.{} => {}?",
                            model.name,
                            nav.value.name,
                            nav_model
                        ));
                    }

                    // Propogate validation of FK Attribute
                    unresolved_nav_props.push((&model.name, nav_model, nav));

                    // One To Many: Person has many Dogs (sql)=> Dog has an fk to  Person
                    // Person must come before Dog in topo order
                    graph.entry(&model.name).or_default().push(nav_model);
                    *in_degree.entry(nav_model).or_insert(0) += 1;
                }
                CidlForeignKeyKind::ManyToMany { unique_id } => {
                    // Validate FK Type
                    let Some(CidlType::Model(fk_model_name)) = nav.value.cidl_type.unwrap_array()
                    else {
                        return Err(anyhow!(
                            "Expected collection of Model type for navigation property {}.{}",
                            model.name,
                            nav.value.name
                        ));
                    };
                    if nav.value.nullable {
                        return Err(anyhow!(
                            "Many To Many navigation property cannot be nullable {}.{}",
                            model.name,
                            nav.value.name
                        ));
                    }

                    // Validate FK Model
                    if !model_lookup.contains_key(fk_model_name.as_str()) {
                        return Err(anyhow!(
                            "Unknown Model for navigation property {}.{} => {}?",
                            model.name,
                            nav.value.name,
                            fk_model_name
                        ));
                    }

                    // Unwrap: safe because `validate_models` halts on missing PK
                    let pk = pk.unwrap();
                    many_to_many
                        .entry(unique_id)
                        .or_default()
                        .model(JunctionModel {
                            model_name: &model.name,
                            model_pk_name: &pk.name,
                            model_pk_type: pk.cidl_type.clone(),
                        })?;
                }
            }
        }
    }

    // Validate 1:M navigation properties
    for (model_name, nav_model, nav) in unresolved_nav_props {
        if let CidlForeignKeyKind::OneToMany { reference } = &nav.foreign_key {
            // Validate the nav props reference is to a valid fk attribute on
            // another model (meaning, it points back to the nav prop's encompassing model)
            if let Some(&fk_model) = model_reference_to_fk_model.get(&(nav_model, reference)) {
                if model_name != fk_model {
                    return Err(anyhow!(
                        "Mismatched types between foreign key and One to Many navigation property ({}.{}) ({}.{})",
                        model_name,
                        nav.value.name,
                        nav_model,
                        reference
                    ));
                }
            } else {
                return Err(anyhow!(
                    "Navigation property {}.{} references {}.{} which does not exist.",
                    model_name,
                    nav.value.name,
                    nav_model,
                    reference
                ));
            }
        }
    }

    // Validate M:M
    let many_to_many_tables: Vec<String> = many_to_many
        .drain()
        .map(|(unique_id, v)| v.build(unique_id))
        .collect::<Result<_, _>>()?;

    // Kahn's algorithm
    let mut queue = in_degree
        .iter()
        .filter_map(|(&name, &deg)| (deg == 0).then_some(name))
        .collect::<VecDeque<_>>();

    let mut ordered = Vec::with_capacity(models.len());
    while let Some(model_name) = queue.pop_front() {
        // Unwrap: safe because graph population covers the entire set
        ordered.push(*model_lookup.get(model_name).unwrap());

        if let Some(adjs) = graph.get(model_name) {
            for adj in adjs {
                // Unwrap: safe because `name_to_model` halts on unknown FK's
                let deg = in_degree.get_mut(adj).unwrap();
                *deg -= 1;

                if *deg == 0 {
                    queue.push_back(adj);
                }
            }
        }
    }

    if ordered.len() != models.len() {
        let cyclic: Vec<&str> = in_degree
            .iter()
            .filter_map(|(&n, &d)| (d > 0).then_some(n))
            .collect();
        return Err(anyhow!("Cycle detected: {}", cyclic.join(", ")));
    }

    Ok((ordered, many_to_many_tables))
}

/// Validates the CIDL Model AST and then maps the models to SQL tables
pub fn generate_sql_tables(models: &[Model]) -> Result<String> {
    let model_lookup = validate_models(models)?;
    let (sorted_models, mut many_to_many_tables) = validate_fks(models, &model_lookup)?;

    let mut res = Vec::new();
    for &model in &sorted_models {
        let mut table = Table::create();
        table.table(Alias::new(model.name.clone()));

        for attr in model.attributes.iter() {
            let mut column = ColumnDef::new(Alias::new(attr.value.name.clone()));
            type_column(&mut column, &attr.value.cidl_type)?;

            if attr.primary_key {
                column.primary_key();
            } else if !attr.value.nullable {
                column.not_null();
            }

            // Set attribute foreign key
            if let Some(fk_model_name) = &attr.foreign_key {
                // Unwrap: safe because `validate_models` and `validate_fks` halt
                // if the values are missing
                let pk_name = &model_lookup
                    .get(fk_model_name.as_str())
                    .unwrap()
                    .find_primary_key()
                    .unwrap()
                    .name;

                table.foreign_key(
                    ForeignKey::create()
                        .from(
                            Alias::new(model.name.clone()),
                            Alias::new(attr.value.name.as_str()),
                        )
                        .to(Alias::new(fk_model_name.clone()), Alias::new(pk_name))
                        .on_update(sea_query::ForeignKeyAction::Cascade)
                        .on_delete(sea_query::ForeignKeyAction::Restrict),
                );
            }

            table.col(column);
        }

        // Generate SQLite
        res.push(format!("{};", table.build(SqliteQueryBuilder)));
    }

    // Add junction tables to the end of the query
    res.append(&mut many_to_many_tables);

    Ok(res.join("\n"))
}
