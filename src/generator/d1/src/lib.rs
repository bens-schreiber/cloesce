use std::collections::{BTreeMap, HashMap, VecDeque};

use common::{
    CidlType, CloesceAst, IncludeTree, Model, NavigationPropertyKind, ensure,
    err::{GeneratorErrorKind, Result},
    fail,
};

use indexmap::IndexMap;
use sea_query::{
    ColumnDef, Expr, ForeignKey, Index, IntoCondition, Query, SelectStatement, SqliteQueryBuilder,
    Table,
};

pub enum DiffKind {
    Refactor,
    Addition,
}

pub struct D1Generator;
impl D1Generator {
    /// Runs semantic analysis on the AST, raising a [GeneratorError] on invalid grammar.
    pub fn validate_ast(ast: &mut CloesceAst) -> Result<()> {
        validate_fks(&mut ast.models)?;
        validate_data_sources(&ast.models)?;
        Ok(())
    }

    /// Runs semantic analysis on the AST, raising a [GeneratorError] on invalid grammar.
    ///
    /// Uses the last migrated [CloesceAst] to produce a new migrated SQL schema.
    pub fn migrate_ast(ast: &mut CloesceAst, lm_ast: Option<&CloesceAst>) -> Result<String> {
        let many_to_many_tables = validate_fks(&mut ast.models)?;
        let model_tree = validate_data_sources(&ast.models)?;

        if let Some(lm_ast) = lm_ast
            && lm_ast.hash == ast.hash
        {
            // No work to be done
            return Ok(String::default());
        }

        let table = MigrateModelTables::make_migrations(ast, lm_ast, many_to_many_tables);
        let view = MigrateDataSources::make_migrations(model_tree, ast, lm_ast);

        Ok(format!("{table}\n{view}"))
    }
}

struct MigrateDataSources;
impl MigrateDataSources {
    /// Takes in a list of [ModelTree] and returns a list of `CREATE VIEW` statements
    /// derived from their respective tree.
    fn create(model_trees: Vec<ModelTree>) -> Vec<String> {
        let mut views = vec![];

        for tree in model_trees {
            let root_model = &tree.root.model;

            let mut query = Query::select();

            query.from(alias(&root_model.name));
            dfs(&tree.root, &mut query, &mut vec![root_model.name.clone()]);

            views.push(format!(
                "CREATE VIEW IF NOT EXISTS \"{}.{}\" AS {};",
                root_model.name,
                tree.name,
                query.to_string(SqliteQueryBuilder)
            ))
        }

        return views;

        fn dfs(node: &ModelTreeNode, query: &mut SelectStatement, path: &mut Vec<String>) {
            let path_to_column = path.join(".");

            // Primary Key
            {
                let pk = &node.model.primary_key.name;
                let col = if matches!(
                    node.parent_transition_kind,
                    Some(NavigationPropertyKind::ManyToMany { .. })
                ) {
                    // M:M pk is in the form "UniqueIdN.ModelName.PrimaryKeyName"
                    Expr::col((
                        alias(node.many_to_many_alias.as_ref().unwrap()),
                        alias(format!("{}.{}", node.model.name, pk)),
                    ))
                } else {
                    Expr::col((alias(&node.model_alias), alias(pk)))
                };

                query.expr_as(col, alias(format!("{}.{}", &path_to_column, pk)));
            }

            // Columns
            for attr in &node.model.attributes {
                query.expr_as(
                    Expr::col((alias(&node.model_alias), alias(&attr.value.name))),
                    alias(format!("{}.{}", &path_to_column, attr.value.name)),
                );
            }

            // Navigation properties
            for child in &node.children {
                match child.parent_transition_kind.as_ref().unwrap() {
                    NavigationPropertyKind::OneToOne { reference } => {
                        let nav_model_pk = &child.model.primary_key.name;

                        left_join_as(
                            query,
                            &child.model.name,
                            &child.model_alias,
                            Expr::col((alias(&node.model_alias), alias(reference)))
                                .equals((alias(&child.model_alias), alias(nav_model_pk))),
                        );
                    }
                    NavigationPropertyKind::OneToMany { reference } => {
                        let pk = &node.model.primary_key.name;

                        left_join_as(
                            query,
                            &child.model.name,
                            &child.model_alias,
                            Expr::col((alias(&node.model_alias), alias(pk)))
                                .equals((alias(&child.model_alias), alias(reference))),
                        );
                    }
                    NavigationPropertyKind::ManyToMany { unique_id } => {
                        let nav_model_pk = &child.model.primary_key;
                        let pk = &node.model.primary_key.name;

                        left_join_as(
                            query,
                            unique_id,
                            child.many_to_many_alias.as_ref().unwrap(),
                            Expr::col((alias(&node.model_alias), alias(pk))).equals((
                                alias(child.many_to_many_alias.as_ref().unwrap()),
                                alias(format!("{}.{}", node.model.name, pk)),
                            )),
                        );

                        left_join_as(
                            query,
                            &child.model.name,
                            &child.model_alias,
                            Expr::col((
                                alias(child.many_to_many_alias.as_ref().unwrap()),
                                alias(format!("{}.{}", child.model.name, pk)),
                            ))
                            .equals((alias(&child.model_alias), alias(&nav_model_pk.name))),
                        );
                    }
                }
                path.push(child.parent_transition_name.as_ref().unwrap().clone());
                dfs(child, query, path);
                path.pop();
            }
        }
    }

    /// Returns a vector of dropped data sources, along with a vector of data sources that need to created.
    ///
    /// If a data source is altered, it will be dropped and added to the create list. Thus [generate_drop_views]
    /// must be queried before [Self::create]
    fn drop<'a>(
        model_trees: Vec<ModelTree<'a>>,
        ast: &CloesceAst,
        lm_lookup: &IndexMap<String, Model>,
    ) -> (Vec<String>, Vec<ModelTree<'a>>) {
        let mut drops = vec![];

        for lm_model in lm_lookup.values() {
            match ast.models.get(&lm_model.name) {
                // Model exists
                Some(model) if model.hash != lm_model.hash => {
                    for lm_ds in lm_model.data_sources.values() {
                        let changed = model
                            .data_sources
                            .get(&lm_ds.name)
                            .map_or(true, |ds| ds.hash != lm_ds.hash);

                        if changed {
                            drops.push(format!(
                                "DROP VIEW IF EXISTS \"{}.{}\";",
                                lm_model.name, lm_ds.name
                            ));
                        }
                    }
                }

                // Model was removed entirely
                None => {
                    for lm_ds in lm_model.data_sources.values() {
                        drops.push(format!(
                            "DROP VIEW IF EXISTS \"{}.{}\";",
                            lm_model.name, lm_ds.name
                        ));
                    }
                }

                // Model unchanged
                _ => {}
            }
        }

        let creates = model_trees
            .into_iter()
            .filter(|tree| {
                lm_lookup
                    .get(&tree.root.model.name)
                    .and_then(|m| m.data_sources.get(&tree.name))
                    .map_or(true, |ds| ds.hash != Some(tree.hash))
            })
            .collect();

        (drops, creates)
    }

    /// Given a vector of all model trees in the new AST, and the last migrated AST's lookup table,
    /// produces a sequence of SQL queries `CREATE`-ing and `DROP`-ing the last migrated AST to
    /// sync with the new.
    ///
    /// TODO: Doesn't produce `ALTER` statements in favor of just dropping the view and creating it again.
    fn make_migrations(
        all_model_trees: Vec<ModelTree>,
        ast: &CloesceAst,
        lm_ast: Option<&CloesceAst>,
    ) -> String {
        let (drop_stmts, create_model_trees) = if let Some(lm_ast) = lm_ast {
            Self::drop(all_model_trees, ast, &lm_ast.models)
        } else {
            // No last migration, insert all
            (Vec::default(), all_model_trees)
        };

        let create_stmts = Self::create(create_model_trees);

        let mut sql_stmt = String::new();

        if !drop_stmts.is_empty() {
            sql_stmt.push_str("--- Dropped and Refactored Data Sources\n");
            sql_stmt.push_str(&drop_stmts.join("\n"));
            sql_stmt.push('\n');
        }

        if !create_stmts.is_empty() {
            sql_stmt.push_str("--- New Data Sources\n");
            sql_stmt.push_str(&create_stmts.join("\n"));
            sql_stmt.push('\n');
        }

        sql_stmt
    }
}

struct MigrateModelTables;
impl MigrateModelTables {
    /// Takes in a vec of models and naively inserts all of them.
    fn create(
        sorted_models: &Vec<&Model>,
        model_lookup: &IndexMap<String, Model>,
        many_to_many_tables: Vec<JunctionTable>,
    ) -> Vec<String> {
        let mut res = Vec::new();

        for &model in sorted_models {
            let mut table = Table::create();
            table.table(alias(model.name.clone()));
            table.if_not_exists();

            // Set Primary Key
            {
                let mut column =
                    typed_column(&model.primary_key.name, &model.primary_key.cidl_type);
                column.primary_key();
                table.col(column);
            }

            for attr in model.attributes.iter() {
                let mut column = typed_column(&attr.value.name, &attr.value.cidl_type);

                if !attr.value.cidl_type.is_nullable() {
                    column.not_null();
                }

                // Set attribute foreign key
                if let Some(fk_model_name) = &attr.foreign_key_reference {
                    // Unwrap: safe because `validate_models` and `validate_fks` halt
                    // if the values are missing
                    let pk_name = &model_lookup
                        .get(fk_model_name.as_str())
                        .unwrap()
                        .primary_key
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
            let col_a_name = format!("{}.{}", a.model_name, a.model_pk_name);
            let mut col_a = typed_column(&col_a_name, &a.model_pk_type);

            let col_b_name = format!("{}.{}", b.model_name, b.model_pk_name);
            let mut col_b = typed_column(&col_b_name, &b.model_pk_type);

            table
                .table(alias(&unique_id))
                .col(col_a.not_null())
                .col(col_b.not_null())
                .primary_key(
                    Index::create()
                        .col(alias(&col_a_name))
                        .col(alias(&col_b_name)),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(alias(&unique_id), alias(&col_a_name))
                        .to(alias(a.model_name), alias(a.model_pk_name))
                        .on_update(sea_query::ForeignKeyAction::Cascade)
                        .on_delete(sea_query::ForeignKeyAction::Restrict),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(alias(&unique_id), alias(&col_b_name))
                        .to(alias(b.model_name), alias(b.model_pk_name))
                        .on_update(sea_query::ForeignKeyAction::Cascade)
                        .on_delete(sea_query::ForeignKeyAction::Restrict),
                );

            res.push(format!("{};", table.to_string(SqliteQueryBuilder)));
        }

        res
    }

    fn alter(
        sorted_models: &[&Model],
        model_lookup: &IndexMap<String, Model>,
        lm_lookup: &IndexMap<String, Model>,
    ) -> Vec<String> {
        Vec::default()
    }

    /// Takes in a vec of last migrated models and deletes all of their m2m tables and tables.
    /// The last migrated models should be sorted topologically in insert order.
    fn drop(sorted_lm_models: &[&Model]) -> Vec<String> {
        let mut res = vec![];

        // The reverse of insert order
        let drop_order = sorted_lm_models.iter().rev();

        for &model in drop_order {
            // Drop M2M's
            for m2m_id in model
                .navigation_properties
                .iter()
                .filter_map(|n| match &n.kind {
                    NavigationPropertyKind::ManyToMany { unique_id } => Some(unique_id),
                    _ => None,
                })
            {
                let mut drop = Table::drop();
                drop.table(alias(m2m_id));
                drop.if_exists();
                res.push(format!("{};", drop.build(SqliteQueryBuilder).to_string()));
            }

            // Drop table
            let mut drop = Table::drop();
            drop.table(alias(&model.name));
            drop.if_exists();
            res.push(format!("{};", drop.build(SqliteQueryBuilder).to_string()));
        }

        res
    }

    /// Given an AST and the last migrated AST, produces a sequence of SQL queries `CREATE`-ing, `DROP`-ing
    /// and `ALTER`-ing the last migrated AST to sync with the new.
    fn make_migrations(
        ast: &CloesceAst,
        lm_ast: Option<&CloesceAst>,
        mut many_to_many_tables: HashMap<String, JunctionTable>,
    ) -> String {
        let Some(lm_ast) = lm_ast else {
            // No previous migration, insert naively.
            return Self::create(
                &ast.models.values().collect(),
                &ast.models,
                many_to_many_tables.into_values().collect(),
            )
            .join("\n");
        };

        let (sorted_create_models, alter_models, sorted_drop_lms) = {
            let mut inserts = vec![];
            let mut alerts = vec![];
            let mut drops = vec![];

            for model in ast.models.values() {
                match lm_ast.models.get(&model.name) {
                    Some(lm_model) => {
                        if lm_model.hash == model.hash {
                            // No changes have been made
                            continue;
                        }

                        alerts.push(model);
                    }
                    None => {
                        inserts.push(model);
                    }
                }
            }

            for lm_model in lm_ast.models.values() {
                if ast.models.get(&lm_model.name).is_none() {
                    drops.push(lm_model);
                    continue;
                }

                for nav in &lm_model.navigation_properties {
                    let NavigationPropertyKind::ManyToMany { unique_id } = &nav.kind else {
                        continue;
                    };

                    // This table already exists and does not need to be added.
                    many_to_many_tables.remove(unique_id.as_str());
                }
            }

            (inserts, alerts, drops)
        };

        let create_stmts = Self::create(
            &sorted_create_models,
            &ast.models,
            many_to_many_tables.into_values().collect(),
        );
        let drop_stmts = Self::drop(&sorted_drop_lms);
        // let alter_stmts = Self::alter(&alter_models, model_lookup, lm_lookup)

        let mut sql_stmt = String::new();

        for (title, stmts) in [
            ("--- New Models\n", &create_stmts),
            // ("--- Altered Models\n", &alter_stmts),
            ("--- Dropped Models\n", &drop_stmts),
        ] {
            if !stmts.is_empty() {
                sql_stmt.push_str(title);
                sql_stmt.push_str(&stmts.join("\n"));
                sql_stmt.push('\n');
            }
        }

        sql_stmt
    }
}

/// Validates foreign key relationships for every [Model] in the AST.
///
/// Modifies the [CloesceAst]'s [IndexMap<String, Model>] to be in topological order.
///
/// Returns error on
/// - Unknown or invalid foreign key references
/// - Missing navigation property attributes
/// - Cyclical dependencies
///
/// Returns a map of all junction table unique ids to their junction table.
fn validate_fks(
    model_lookup: &mut IndexMap<String, Model>,
) -> Result<HashMap<String, JunctionTable>> {
    // Topo sort and cycle detection
    let mut in_degree = BTreeMap::<&str, usize>::new();
    let mut graph = BTreeMap::<&str, Vec<&str>>::new();

    let mut many_to_many = HashMap::<&String, JunctionTableBuilder>::new();

    // Maps a model name and a foreign key reference to the model it is referencing
    // Ie, Person.dogId => { (Person, dogId): "Dog" }
    let mut model_reference_to_fk_model = HashMap::<(&str, &str), &str>::new();
    let mut unvalidated_navs = Vec::new();

    for model in model_lookup.values() {
        graph.entry(&model.name).or_default();
        in_degree.entry(&model.name).or_insert(0);

        for attr in &model.attributes {
            let Some(fk_model) = &attr.foreign_key_reference else {
                continue;
            };

            model_reference_to_fk_model.insert((&model.name, attr.value.name.as_str()), fk_model);

            // Nullable FK's do not constrain table creation order, and thus
            // can be left out of the topo sort
            if !attr.value.cidl_type.is_nullable() {
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
                    // Validate the nav prop's reference is consistent
                    if let Some(&fk_model) =
                        model_reference_to_fk_model.get(&(&model.name, reference))
                    {
                        ensure!(
                            fk_model == nav.model_name,
                            GeneratorErrorKind::MismatchedNavigationPropertyTypes,
                            "({}.{}) does not match type ({})",
                            model.name,
                            nav.var_name,
                            fk_model
                        );
                    } else {
                        fail!(
                            GeneratorErrorKind::InvalidNavigationPropertyReference,
                            "{}.{} references {}.{} which does not exist or is not a foreign key to {}",
                            model.name,
                            nav.var_name,
                            nav.model_name,
                            reference,
                            model.name
                        );
                    }
                }
                NavigationPropertyKind::OneToMany { reference: _ } => {
                    unvalidated_navs.push((&model.name, &nav.model_name, nav));
                }
                NavigationPropertyKind::ManyToMany { unique_id } => {
                    many_to_many
                        .entry(unique_id)
                        .or_default()
                        .model(JunctionModel {
                            model_name: model.name.clone(),
                            model_pk_name: model.primary_key.name.clone(),
                            model_pk_type: model.primary_key.cidl_type.clone(),
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
            fail!(
                GeneratorErrorKind::InvalidNavigationPropertyReference,
                "{}.{} references {}.{} which does not exist or is not a foreign key to {}",
                model_name,
                nav.var_name,
                nav_model,
                reference,
                model_name
            );
        };

        // The types should reference one another
        // ie, Person has many dogs, personId on dog should be an fk to Person
        ensure!(
            model_name == fk_model,
            GeneratorErrorKind::MismatchedNavigationPropertyTypes,
            "({}.{}) does not match type ({}.{})",
            model_name,
            nav.var_name,
            nav_model,
            reference,
        );

        // One To Many: Person has many Dogs (sql)=> Dog has an fk to  Person
        // Person must come before Dog in topo order
        graph.entry(model_name).or_default().push(nav_model);
        *in_degree.entry(nav_model).or_insert(0) += 1;
    }

    // Validate M:M
    let many_to_many_tables: HashMap<String, JunctionTable> = many_to_many
        .drain()
        .map(|(unique_id, v)| Ok((unique_id.to_owned(), v.build(unique_id)?)))
        .collect::<Result<_>>()?;

    // Kahn's algorithm
    let rank = {
        let mut queue = in_degree
            .iter()
            .filter_map(|(&name, &deg)| (deg == 0).then_some(name))
            .collect::<VecDeque<_>>();

        let mut rank = HashMap::with_capacity(model_lookup.len());
        let mut counter = 0usize;

        while let Some(model_name) = queue.pop_front() {
            rank.insert(model_name.to_string(), counter);
            counter += 1;

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

        if rank.len() != model_lookup.len() {
            let cyclic: Vec<&str> = in_degree
                .iter()
                .filter_map(|(&n, &d)| (d > 0).then_some(n))
                .collect();
            fail!(
                GeneratorErrorKind::CyclicalModelDependency,
                "{}",
                cyclic.join(", ")
            );
        }

        rank
    };
    model_lookup.sort_by_key(|k, _| rank.get(k.as_str()).unwrap());

    Ok(many_to_many_tables)
}

struct ModelTreeNode<'a> {
    parent_transition_kind: Option<NavigationPropertyKind>,
    parent_transition_name: Option<String>,

    /// The alias of the associated [Model], being some form of
    /// "ModelName" + N, where N is how many times the model occurs in the tree.
    model_alias: String,

    /// The alias of the associated many to many table if it exists, being
    /// some form of "UniqueId" + N, where N is how many times the model occurs in the tree.
    many_to_many_alias: Option<String>,
    model: &'a Model,
    children: Vec<ModelTreeNode<'a>>,
}

/// A tree of models derived from a [Model]'s data source include tree.
struct ModelTree<'a> {
    name: String,
    hash: u64,
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
    model_lookup: &'a IndexMap<String, Model>,
) -> Result<Vec<ModelTree<'a>>> {
    let mut model_trees = vec![];

    for model in model_lookup.values() {
        for ds in model.data_sources.values() {
            let mut alias_counter = HashMap::<String, u32>::new();

            let tree = dfs(
                model,
                None,
                None,
                &ds.tree,
                model_lookup,
                &mut alias_counter,
            )
            .map_err(|e| {
                e.with_context(format!(
                    "Problem found while validating data source {}.{}",
                    model.name, ds.name
                ))
            })?;

            model_trees.push(ModelTree {
                name: ds.name.clone(),
                root: tree,
                hash: ds.hash.unwrap(),
            })
        }
    }

    fn dfs<'a>(
        model: &'a Model,
        transition: Option<NavigationPropertyKind>,
        transition_name: Option<String>,
        include_tree: &IncludeTree,
        model_lookup: &'a IndexMap<String, Model>,
        alias_counter: &mut HashMap<String, u32>,
    ) -> Result<ModelTreeNode<'a>> {
        let model_alias = generate_alias(&model.name, alias_counter);
        let many_to_many_alias = transition.clone().and_then(|m| match &m {
            NavigationPropertyKind::ManyToMany { unique_id } => {
                Some(generate_alias(unique_id, alias_counter))
            }
            _ => None,
        });

        let mut node = ModelTreeNode {
            parent_transition_kind: transition,
            parent_transition_name: transition_name,
            model_alias: model_alias.clone(),
            model,
            many_to_many_alias,
            children: vec![],
        };

        for (var_name, child_tree) in &include_tree.0 {
            // Referenced attribute must exist
            let Some(nav) = model
                .navigation_properties
                .iter()
                .find(|nav| nav.var_name == *var_name)
            else {
                fail!(
                    GeneratorErrorKind::UnknownIncludeTreeReference,
                    "{}.{}",
                    model.name,
                    var_name
                )
            };

            // Validate model exists
            let child_model = model_lookup
                .get(nav.model_name.as_str())
                .expect("model names to be validated");

            let child_node = dfs(
                child_model,
                Some(nav.kind.clone()),
                Some(nav.var_name.clone()),
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
            format!("{}{}", name, count)
        };
        *count += 1;
        alias
    }
}

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
    let inner = match ty {
        CidlType::Nullable(inner) => inner.as_ref(),
        t => t,
    };

    match inner {
        CidlType::Integer => col.integer(),
        CidlType::Real => col.decimal(),
        CidlType::Text => col.text(),
        CidlType::Blob => col.blob(),
        _ => unreachable!("column type must be validated earlier"),
    };
    col
}

/// Represents one side of a Many to Many junction table
struct JunctionModel {
    model_name: String,
    model_pk_name: String,
    model_pk_type: CidlType,
}

/// A full Many to Many table with both sides
struct JunctionTable {
    a: JunctionModel,
    b: JunctionModel,
    unique_id: String,
}

#[derive(Default)]
struct JunctionTableBuilder {
    models: Vec<JunctionModel>,
}

impl JunctionTableBuilder {
    fn model(&mut self, jm: JunctionModel) -> Result<()> {
        if self.models.len() >= 2 {
            fail!(
                GeneratorErrorKind::ExtraneousManyToManyReferences,
                "{}, {}, {}",
                jm.model_name,
                self.models[0].model_name,
                self.models[1].model_name
            );
        }
        self.models.push(jm);
        Ok(())
    }

    fn build(self, unique_id: &str) -> Result<JunctionTable> {
        let err_context = self
            .models
            .first()
            .map(|m| m.model_name.clone())
            .unwrap_or_default();
        let [a, b] = <[_; 2]>::try_from(self.models).map_err(|_| {
            GeneratorErrorKind::MissingManyToManyReference
                .to_error()
                .with_context(err_context)
        })?;

        Ok(JunctionTable {
            a,
            b,
            unique_id: unique_id.to_string(),
        })
    }
}
