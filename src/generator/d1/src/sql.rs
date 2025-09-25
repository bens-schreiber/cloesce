use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use common::{
    CidlType, IncludeTree, Model, NamedTypedValue, NavigationProperty, NavigationPropertyKind,
};

use anyhow::{Context, Result, anyhow, bail, ensure};
use sea_query::{
    ColumnDef, Expr, ForeignKey, Index, IntoCondition, Query, SelectStatement, SqliteQueryBuilder,
    Table,
};

// TODO: SeaQuery forcing us to do alias everywhere is really annoying,
// it feels like this library is a bad choice for our use case.
fn alias(name: impl Into<String>) -> sea_query::Alias {
    sea_query::Alias::new(name)
}

fn left_join_as(
    query: &mut SelectStatement,
    model_name: &str,
    model_alias: &str,
    condition: impl IntoCondition,
) {
    if model_name == model_alias {
        query.left_join(alias(model_name), condition);
    } else {
        query.join_as(
            sea_query::JoinType::LeftJoin,
            alias(model_name),
            alias(model_alias),
            condition,
        );
    }
}

fn typed_column(name: &str, ty: &CidlType) -> ColumnDef {
    let mut col = ColumnDef::new(alias(name));
    match ty {
        CidlType::Integer => col.integer(),
        CidlType::Real => col.decimal(),
        CidlType::Text => col.text(),
        CidlType::Blob => col.blob(),
        _ => unreachable!("column type must be validated earlier"),
    };
    col
}

/// Represents one side of a Many to Many junction table
struct JunctionModel<'a> {
    model_name: &'a str,
    model_pk_name: &'a str,
    model_pk_type: CidlType,
}

/// A full Many to Many table with both sides
struct JunctionTable<'a> {
    a: JunctionModel<'a>,
    b: JunctionModel<'a>,
    unique_id: &'a str,
}

#[derive(Default)]
struct JunctionTableBuilder<'a> {
    models: Vec<JunctionModel<'a>>,
}

impl<'a> JunctionTableBuilder<'a> {
    fn model(&mut self, jm: JunctionModel<'a>) -> Result<()> {
        if self.models.len() >= 2 {
            bail!("Too many ManyToMany navigation properties for junction table");
        }
        self.models.push(jm);
        Ok(())
    }

    fn build(self, unique_id: &'a str) -> Result<JunctionTable<'a>> {
        let [a, b] = <[_; 2]>::try_from(self.models)
            .map_err(|_| anyhow!("Both models must be set for a junction table"))?;
        Ok(JunctionTable { a, b, unique_id })
    }
}

fn validate_nav_array<'a>(
    model: &Model,
    nav: &'a NavigationProperty,
    model_lookup: &HashMap<&str, &Model>,
) -> Result<&'a str> {
    let Some(CidlType::Model(model_name)) = nav.value.cidl_type.unwrap_array() else {
        bail!(
            "Expected collection of Model type for navigation property {}.{}",
            model.name,
            nav.value.name
        );
    };

    if nav.value.nullable {
        bail!(
            "Navigation property cannot be nullable {}.{}",
            model.name,
            nav.value.name
        );
    }

    if !model_lookup.contains_key(model_name.as_str()) {
        bail!(
            "Unknown Model for navigation property {}.{} => {}?",
            model.name,
            nav.value.name,
            model_name
        );
    }

    Ok(model_name.as_str())
}

/// Validates all models, returning a lookup table of a model name to it's [Model].
///
/// Returns error on
/// - Duplicate Model names
/// - Duplicate column names
/// - Duplicate primary keys
/// - Invalid typed primary keys
/// - Missing primary keys
/// - Invalid SQL column type
fn validate_models(models: &[Model]) -> Result<HashMap<&str, &Model>> {
    let mut model_lookup = HashMap::<&str, &Model>::new();
    for model in models {
        // Duplicate models
        ensure!(
            model_lookup.insert(&model.name, model).is_none(),
            "Duplicate model name: {}",
            model.name
        );

        let mut column_names = HashSet::new();
        let mut has_pk = false;
        for attr in &model.attributes {
            // Duplicate columns
            ensure!(
                column_names.insert(&attr.value.name),
                "Duplicate column names {}.{}",
                model.name,
                attr.value.name
            );

            // Validate primary key
            if attr.is_primary_key {
                ensure!(!has_pk, "Duplicate primary keys on model {}", model.name);
                ensure!(!attr.value.nullable, "A primary key cannot be nullable.");
                ensure!(
                    attr.foreign_key_reference.is_none(),
                    "A primary key cannot be a foreign key"
                );
                has_pk = true;
            }

            ensure!(
                matches!(
                    attr.value.cidl_type,
                    CidlType::Integer | CidlType::Real | CidlType::Text | CidlType::Blob
                ),
                "Invalid SQL Type {}.{}",
                model.name,
                attr.value.name
            );
        }

        ensure!(has_pk, "Missing primary key on model {}", model.name);
    }

    Ok(model_lookup)
}

/// Validates foreign key relationships for every [Model] in the CIDL. Returns a
/// SQL-safe topological ordering of the models, along with a vec of all necessary Junction tables
///
/// Returns error on
/// - Unknown foreign key models
/// - Invalid navigation property types
/// - Missing navigation property attributes
/// - Cyclical dependencies
fn validate_fks<'a>(
    models: &'a [Model],
    model_lookup: &HashMap<&str, &'a Model>,
) -> Result<(Vec<&'a Model>, Vec<JunctionTable<'a>>)> {
    // Topo sort and cycle detection
    let mut in_degree = BTreeMap::<&str, usize>::new();
    let mut graph = BTreeMap::<&str, Vec<&str>>::new();

    let mut many_to_many = HashMap::<&String, JunctionTableBuilder>::new();

    // Maps a model name and a foreign key reference to the model it is referencing
    // Ie, Person.dogId => { (Person, dogId): "Dog" }
    let mut model_reference_to_fk_model = HashMap::<(&str, &str), &str>::new();
    let mut unvalidated_navs = Vec::new();

    for model in models {
        graph.entry(&model.name).or_default();
        in_degree.entry(&model.name).or_insert(0);

        let mut pk: Option<&NamedTypedValue> = None;
        for attr in &model.attributes {
            let Some(fk_model) = &attr.foreign_key_reference else {
                if attr.is_primary_key {
                    pk = Some(&attr.value);
                }
                continue;
            };

            // Validate the fk's model exists
            ensure!(
                model_lookup.contains_key(fk_model.as_str()),
                "Unknown Model for foreign key {}.{} => {}?",
                model.name,
                attr.value.name,
                fk_model
            );

            model_reference_to_fk_model.insert((&model.name, attr.value.name.as_str()), fk_model);

            // Nullable FK's do not constrain table creation order, and thus
            // can be left out of the topo sort
            if !attr.value.nullable {
                // One To One: Person has a Dog ..(sql)=> Person has a fk to Dog
                // Dog must come before Person
                graph.entry(fk_model).or_default().push(&model.name);
                in_degree.entry(&model.name).and_modify(|d| *d += 1);
            }
        }

        // Validate navigation property types
        for nav in &model.navigation_properties {
            match &nav.kind {
                NavigationPropertyKind::OneToOne { reference } => {
                    // Validate nav prop has a Model type
                    let CidlType::Model(nav_model) = &nav.value.cidl_type else {
                        bail!(
                            "Expected Model type for navigation property {}.{}",
                            model.name,
                            nav.value.name
                        );
                    };

                    // Validate the nav prop's model exists
                    ensure!(
                        model_lookup.contains_key(nav_model.as_str()),
                        "Unknown Model for navigation property {}.{} => {}?",
                        model.name,
                        nav.value.name,
                        nav_model
                    );

                    // Validate the nav prop's reference is consistent
                    if let Some(&fk_model) =
                        model_reference_to_fk_model.get(&(&model.name, reference))
                    {
                        ensure!(
                            fk_model == nav_model,
                            "Mismatched types between foreign key and One to One navigation property ({}.{}) ({})",
                            model.name,
                            nav.value.name,
                            fk_model
                        );
                    } else {
                        bail!(
                            "Navigation property {}.{} references {}.{} which does not exist.",
                            model.name,
                            nav.value.name,
                            nav_model,
                            reference
                        );
                    }

                    // TODO: Revisit this. Should a user be able to decorate a One To One
                    // navigation property, but have no foreign key for it?
                    // ( ie, make the enum OneToOne(Option<String>) )
                }
                NavigationPropertyKind::OneToMany { reference: _ } => {
                    let nav_model = validate_nav_array(model, nav, model_lookup)?;
                    unvalidated_navs.push((&model.name, nav_model, nav));
                }
                NavigationPropertyKind::ManyToMany { unique_id } => {
                    validate_nav_array(model, nav, model_lookup)?;

                    let pk = pk.expect("safe beause validate_models halts on missing pk");
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

    // Validate 1:M nav props
    for (model_name, nav_model, nav) in unvalidated_navs {
        let NavigationPropertyKind::OneToMany { reference } = &nav.kind else {
            continue;
        };

        // Validate the nav props reference is consistent to an attribute
        // on another model
        let Some(&fk_model) = model_reference_to_fk_model.get(&(nav_model, reference)) else {
            bail!(
                "Navigation property {}.{} references {}.{} which does not exist.",
                model_name,
                nav.value.name,
                nav_model,
                reference
            );
        };

        // The types should reference one another
        // ie, Person has many dogs, personId on dog should be an fk to Person
        ensure!(
            model_name == fk_model,
            "Mismatched types between foreign key and One to Many navigation property ({}.{}) ({}.{})",
            model_name,
            nav.value.name,
            nav_model,
            reference,
        );

        // One To Many: Person has many Dogs (sql)=> Dog has an fk to  Person
        // Person must come before Dog in topo order
        graph.entry(model_name).or_default().push(nav_model);
        *in_degree.entry(nav_model).or_insert(0) += 1;
    }

    // Validate M:M
    let many_to_many_tables: Vec<JunctionTable> = many_to_many
        .drain()
        .map(|(unique_id, v)| v.build(unique_id))
        .collect::<Result<_>>()?;

    // Kahn's algorithm
    let topo_sorted = {
        let mut queue = in_degree
            .iter()
            .filter_map(|(&name, &deg)| (deg == 0).then_some(name))
            .collect::<VecDeque<_>>();

        let mut sorted = Vec::with_capacity(models.len());
        while let Some(model_name) = queue.pop_front() {
            sorted.push(
                *model_lookup
                    .get(model_name)
                    .expect("model names to be validated"),
            );

            if let Some(adjs) = graph.get(model_name) {
                for adj in adjs {
                    let deg = in_degree.get_mut(adj).expect("model names to be validated");
                    *deg -= 1;

                    if *deg == 0 {
                        queue.push_back(adj);
                    }
                }
            }
        }

        if sorted.len() != models.len() {
            let cyclic: Vec<&str> = in_degree
                .iter()
                .filter_map(|(&n, &d)| (d > 0).then_some(n))
                .collect();
            bail!("Cycle detected: {}", cyclic.join(", "));
        }

        sorted
    };

    Ok((topo_sorted, many_to_many_tables))
}

struct ModelTreeNode<'a> {
    parent_transition: Option<NavigationPropertyKind>,
    alias: String,
    model: &'a Model,
    children: Vec<ModelTreeNode<'a>>,
}

struct ModelTree<'a> {
    name: String,
    root: ModelTreeNode<'a>,
}

/// Validates all data sources, ensuring types and references check out.
/// Returns an intermediate [ModelTree] which houses both the models and their
/// SQL correct aliases (Foo vs Foo1 vs Foo2).
///
/// Returns error on
/// - Invalid data source type
/// - Invalid data source reference
/// - Unknown model
fn validate_data_sources<'a>(
    models: &'a [Model],
    model_lookup: &HashMap<&str, &'a Model>,
) -> Result<Vec<ModelTree<'a>>> {
    let mut model_trees = vec![];

    for model in models {
        for ds in &model.data_sources {
            let mut alias_counter = HashMap::<String, u32>::new();

            let tree =
                dfs(model, None, &ds.tree, model_lookup, &mut alias_counter).context(format!(
                    "Problem found while validating data source {}.{}",
                    model.name, ds.name
                ))?;

            model_trees.push(ModelTree {
                name: ds.name.clone(),
                root: tree,
            })
        }
    }

    fn dfs<'a>(
        model: &'a Model,
        transition: Option<NavigationPropertyKind>,
        include_tree: &IncludeTree,
        model_lookup: &HashMap<&str, &'a Model>,
        alias_counter: &mut HashMap<String, u32>,
    ) -> Result<ModelTreeNode<'a>> {
        let alias = generate_alias(&model.name, alias_counter);

        let mut node = ModelTreeNode {
            parent_transition: transition,
            alias: alias.clone(),
            model,
            children: vec![],
        };

        for (value, child_tree) in &include_tree.0 {
            // Validate cidl type
            let nav_model_name = match &value.cidl_type {
                CidlType::Model(m) => m,
                CidlType::Array(inner) => {
                    if let CidlType::Model(m) = &**inner {
                        m
                    } else {
                        bail!("Data Sources must be composed of model references")
                    }
                }
                _ => bail!("Data Sources must be composed of model references"),
            };

            // todo: why even give the option??
            ensure!(!value.nullable, "Data Sources cannot be nullable");

            // Referenced attribute must exist
            let Some(nav) = model
                .navigation_properties
                .iter()
                .find(|nav| nav.value.name == value.name && nav.value.cidl_type == value.cidl_type)
            else {
                bail!("Invalid reference {}.{}", model.name, value.name)
            };

            // Validate model exists
            let child_model = model_lookup
                .get(nav_model_name.as_str())
                .ok_or(anyhow!("Unknown model reference {}", nav_model_name))?;

            let child_node = dfs(
                child_model,
                Some(nav.kind.clone()),
                child_tree,
                model_lookup,
                alias_counter,
            )?;
            node.children.push(child_node);
        }

        Ok(node)
    }

    return Ok(model_trees);

    fn generate_alias(name: &str, alias_counter: &mut HashMap<String, u32>) -> String {
        let count = alias_counter.entry(name.to_string()).or_default();
        let alias = if *count == 0 {
            name.to_string()
        } else {
            format!("{}_{}", name, count)
        };
        *count += 1;
        alias
    }
}

fn generate_tables(
    sorted_models: &[&Model],
    many_to_many_tables: Vec<JunctionTable>,
    model_lookup: &HashMap<&str, &Model>,
) -> Vec<String> {
    let mut res = Vec::new();
    for &model in sorted_models {
        let mut table = Table::create();
        table.table(alias(model.name.clone()));

        for attr in model.attributes.iter() {
            let mut column = typed_column(&attr.value.name, &attr.value.cidl_type);

            if attr.is_primary_key {
                column.primary_key();
            } else if !attr.value.nullable {
                column.not_null();
            }

            // Set attribute foreign key
            if let Some(fk_model_name) = &attr.foreign_key_reference {
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
                        .from(alias(model.name.clone()), alias(attr.value.name.as_str()))
                        .to(alias(fk_model_name.clone()), alias(pk_name))
                        .on_update(sea_query::ForeignKeyAction::Cascade)
                        .on_delete(sea_query::ForeignKeyAction::Restrict),
                );
            }

            table.col(column);
        }

        // Generate SQLite
        res.push(format!("{};", table.build(SqliteQueryBuilder)));
    }

    for JunctionTable { a, b, unique_id } in many_to_many_tables {
        let mut table = Table::create();

        // TODO: Name the junction table in some standard way
        let col_a_name = format!("{}_{}", a.model_name, a.model_pk_name);
        let mut col_a = typed_column(&col_a_name, &a.model_pk_type);

        let col_b_name = format!("{}_{}", b.model_name, b.model_pk_name);
        let mut col_b = typed_column(&col_b_name, &b.model_pk_type);

        table
            .table(alias(unique_id))
            .col(col_a.not_null())
            .col(col_b.not_null())
            .primary_key(
                Index::create()
                    .col(alias(&col_a_name))
                    .col(alias(&col_b_name)),
            )
            .foreign_key(
                ForeignKey::create()
                    .from(alias(unique_id), alias(&col_a_name))
                    .to(alias(a.model_name), alias(a.model_pk_name))
                    .on_update(sea_query::ForeignKeyAction::Cascade)
                    .on_delete(sea_query::ForeignKeyAction::Restrict),
            )
            .foreign_key(
                ForeignKey::create()
                    .from(alias(unique_id), alias(&col_b_name))
                    .to(alias(b.model_name), alias(b.model_pk_name))
                    .on_update(sea_query::ForeignKeyAction::Cascade)
                    .on_delete(sea_query::ForeignKeyAction::Restrict),
            );

        res.push(format!("{};", table.to_string(SqliteQueryBuilder)));
    }

    res
}

fn generate_views(model_tree: Vec<ModelTree>) -> Vec<String> {
    let mut views = vec![];

    for tree in model_tree {
        let mut query = Query::select();

        query.from(alias(&tree.root.model.name));
        dfs(&tree.root, &mut query);

        views.push(format!(
            "CREATE VIEW \"{}_{}\" AS {};",
            tree.root.model.name,
            tree.name,
            query.to_string(SqliteQueryBuilder)
        ))
    }

    return views;

    fn dfs(node: &ModelTreeNode, query: &mut SelectStatement) {
        for attr in &node.model.attributes {
            query.expr_as(
                Expr::col((alias(&node.alias), alias(&attr.value.name))),
                alias(format!("{}_{}", &node.alias, attr.value.name)),
            );
        }

        for child in &node.children {
            match child.parent_transition.as_ref().unwrap() {
                NavigationPropertyKind::OneToOne { reference } => {
                    let nav_model_pk = &child
                        .model
                        .find_primary_key()
                        .expect("primary key to be validated by `validate_models`")
                        .name;

                    left_join_as(
                        query,
                        &child.model.name,
                        &child.alias,
                        Expr::col((alias(&node.alias), alias(reference)))
                            .equals((alias(&child.alias), alias(nav_model_pk))),
                    );
                }
                NavigationPropertyKind::OneToMany { reference } => {
                    let pk = &node
                        .model
                        .find_primary_key()
                        .expect("primary key to be validated by `validate_models`")
                        .name;

                    left_join_as(
                        query,
                        &child.model.name,
                        &child.alias,
                        Expr::col((alias(&node.alias), alias(pk)))
                            .equals((alias(&child.alias), alias(reference))),
                    );
                }
                NavigationPropertyKind::ManyToMany { unique_id } => {
                    let nav_model_pk = child
                        .model
                        .find_primary_key()
                        .expect("primary key to be validated by `validate_models`");

                    let pk = &node
                        .model
                        .find_primary_key()
                        .expect("primary key to be validated by `validate_models`")
                        .name;

                    query.left_join(
                        alias(unique_id),
                        Expr::col((alias(&node.alias), alias(pk)))
                            .equals((alias(unique_id), alias(format!("{}_{}", node.alias, pk)))),
                    );

                    left_join_as(
                        query,
                        &child.model.name,
                        &child.alias,
                        Expr::col((alias(unique_id), alias(format!("{}_{}", child.alias, pk))))
                            .equals((alias(&child.alias), alias(&nav_model_pk.name))),
                    );
                }
            }
            dfs(child, query);
        }
    }
}

/// Validates the Model AST, producing an equivalent sql schema of
/// tables and views
pub fn generate_sql(models: &[Model]) -> Result<String> {
    let model_lookup = validate_models(models)?;
    let (sorted_models, many_to_many_tables) = validate_fks(models, &model_lookup)?;
    let model_tree = validate_data_sources(models, &model_lookup)?;

    let tables = generate_tables(&sorted_models, many_to_many_tables, &model_lookup);
    let views = generate_views(model_tree);

    Ok(format!("{}\n{}", tables.join("\n"), views.join("\n")))
}
