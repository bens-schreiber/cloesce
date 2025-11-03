use std::collections::{BTreeMap, HashMap, VecDeque};

use common::{
    CidlType, CloesceAst, IncludeTree, Model, ModelAttribute, NavigationPropertyKind, ensure,
    err::{GeneratorErrorKind, Result},
    fail,
};

use indexmap::IndexMap;
use sea_query::{
    ColumnDef, Expr, ForeignKey, Index, IntoCondition, Query, SchemaStatementBuilder,
    SelectStatement, SqliteQueryBuilder, Table,
};

pub struct D1Generator;
impl D1Generator {
    /// Runs semantic analysis on the AST, raising a [GeneratorError] on invalid grammar.
    ///
    /// Sorts [CloesceAst::models] to be in topo insertion order.
    pub fn validate_ast(ast: &mut CloesceAst) -> Result<()> {
        Self::validate_fks(&mut ast.models)?;
        Self::validate_data_sources(&ast.models)?;
        Ok(())
    }

    /// Runs [Self::validate_ast] producing a [GeneratorError] on invalid AST grammar.
    ///
    /// Uses the last migrated [CloesceAst] to produce a new migrated SQL schema.
    ///
    /// Some migration scenarios require user intervention through a [MigrationsIntent], which
    /// can be blocking.
    pub fn migrate(
        ast: &mut CloesceAst,
        lm_ast: Option<&CloesceAst>,
        intent: &dyn MigrationsIntent,
    ) -> Result<String> {
        let many_to_many_tables = Self::validate_fks(&mut ast.models)?;
        let model_tree = Self::validate_data_sources(&ast.models)?;

        if let Some(lm_ast) = lm_ast
            && lm_ast.hash == ast.hash
        {
            // No work to be done
            return Ok(String::default());
        }

        let tables = MigrateTables::make_migrations(ast, lm_ast, many_to_many_tables, intent);
        let views = MigrateViews::make_migrations(model_tree, ast, lm_ast);

        Ok(format!("{tables}\n{views}"))
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

                model_reference_to_fk_model
                    .insert((&model.name, attr.value.name.as_str()), fk_model);

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
}

pub enum MigrationsDilemma<'a> {
    RenameOrDropModel {
        model_name: String,
        options: &'a Vec<&'a String>,
    },
    RenameOrDropAttribute {
        model_name: String,
        attribute_name: String,
        options: &'a Vec<&'a String>,
    },
}

pub trait MigrationsIntent {
    /// A potentially blocking call to await some response to the given [MigrationDilemma]
    ///
    /// Returns None if the model should be dropped, Some if an option presented should be selected.
    fn ask(&self, dilemma: MigrationsDilemma) -> Option<usize>;
}

struct MigrateViews;
impl MigrateViews {
    /// Takes in a list of [ModelTree] and returns a list of `CREATE VIEW` statements
    /// derived from their respective tree.
    fn create(model_trees: Vec<ModelTree>) -> Vec<String> {
        let mut res = vec![];

        for tree in model_trees {
            let root_model = &tree.root.model;
            let mut query = Query::select();
            query.from(alias(&root_model.name));

            dfs(&tree.root, &mut query, &mut vec![root_model.name.clone()]);

            res.push(format!(
                "CREATE VIEW IF NOT EXISTS \"{}.{}\" AS {};",
                root_model.name,
                tree.name,
                query.to_string(SqliteQueryBuilder)
            ))
        }

        return res;

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
    }

    /// Returns a list of dropped data sources, along with a list of data sources that need to created.
    ///
    /// If a data source is altered in any way, it will be dropped and added to the create list.
    fn drop<'a>(
        model_trees: Vec<ModelTree<'a>>,
        ast: &CloesceAst,
        lm_lookup: &IndexMap<String, Model>,
    ) -> (Vec<String>, Vec<ModelTree<'a>>) {
        let mut res = vec![];

        for lm_model in lm_lookup.values() {
            match ast.models.get(&lm_model.name) {
                // Model exists from last migration
                Some(model) if model.hash != lm_model.hash => {
                    for lm_ds in lm_model.data_sources.values() {
                        let changed = model
                            .data_sources
                            .get(&lm_ds.name)
                            .is_none_or(|ds| ds.hash != lm_ds.hash);

                        if changed {
                            res.push(format!(
                                "DROP VIEW IF EXISTS \"{}.{}\";",
                                lm_model.name, lm_ds.name
                            ));
                        }
                    }
                }
                // Last migration model was removed entirely
                None => {
                    for lm_ds in lm_model.data_sources.values() {
                        res.push(format!(
                            "DROP VIEW IF EXISTS \"{}.{}\";",
                            lm_model.name, lm_ds.name
                        ));
                    }
                }
                // Last migration model unchanged
                _ => {}
            }
        }

        let creates = model_trees
            .into_iter()
            .filter(|tree| {
                lm_lookup
                    .get(&tree.root.model.name)
                    .and_then(|m| m.data_sources.get(&tree.name))
                    .is_none_or(|ds| ds.hash != Some(tree.hash))
            })
            .collect();

        (res, creates)
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

        let mut res = String::new();
        if !drop_stmts.is_empty() {
            res.push_str("--- Dropped and Refactored Data Sources\n");
            res.push_str(&drop_stmts.join("\n"));
            res.push('\n');
        }
        if !create_stmts.is_empty() {
            res.push_str("--- New Data Sources\n");
            res.push_str(&create_stmts.join("\n"));
            res.push('\n');
        }

        res
    }
}

struct MigrateTables;
impl MigrateTables {
    /// Takes in a list of models and junction tables, generating a list
    /// of naive insert queries.
    fn create(
        sorted_models: Vec<&Model>,
        model_lookup: &IndexMap<String, Model>,
        many_to_many_tables: Vec<JunctionTable>,
    ) -> Vec<String> {
        let mut res = vec![];

        for model in sorted_models {
            let mut table = Table::create();
            table.table(alias(&model.name));
            table.if_not_exists();

            // Set Primary Key
            {
                let mut column =
                    typed_column(&model.primary_key.name, &model.primary_key.cidl_type, false);
                column.primary_key();
                table.col(column);
            }

            // Attributes
            for attr in model.attributes.iter() {
                let mut column = typed_column(&attr.value.name, &attr.value.cidl_type, false);

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

            res.push(to_sqlite(table));
        }

        for JunctionTable { a, b, unique_id } in many_to_many_tables {
            let mut table = Table::create();

            // TODO: Name the junction table in some standard way
            let col_a_name = format!("{}.{}", a.model_name, a.model_pk_name);
            let mut col_a = typed_column(&col_a_name, &a.model_pk_type, false);

            let col_b_name = format!("{}.{}", b.model_name, b.model_pk_name);
            let mut col_b = typed_column(&col_b_name, &b.model_pk_type, false);

            table
                .table(alias(&unique_id))
                .if_not_exists()
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

            res.push(to_sqlite(table));
        }

        res
    }

    /// Generates a sequence of alter statements from a models last migration.
    ///
    /// Some alterations cannot occur in SQLite without dropping the table, in which a
    /// full rebuild and copy of data will occur.
    ///
    /// Poses a [MigrationsDilemma::RenameOrDropModel], determining if a dropped model is
    /// actually just a rename. If that is the case, removes from `drop` and `add` lists, undergoing
    /// table alteration on the (model, last migrated model) pair.
    fn alter<'a>(
        alter_models: Vec<(&'a Model, &'a Model)>,
        model_lookup: &IndexMap<String, Model>,
        intent: &dyn MigrationsIntent,
    ) -> Vec<String> {
        let mut res = vec![];
        let mut rebuilds = vec![];

        'models: for (model, lm_model) in alter_models {
            let mut stmts = vec![];

            // Rename table
            if model.name != lm_model.name {
                let mut rename = Table::rename();
                rename.table(alias(&lm_model.name), alias(&model.name));
                stmts.push(to_sqlite(rename));
            }

            // Alter primary key, requires rebuild.
            if model.primary_key.cidl_type != lm_model.primary_key.cidl_type
                || model.primary_key.name != lm_model.primary_key.name
            {
                rebuilds.push((model, lm_model));
                continue;
            }

            let mut lm_model_attr_lookup = lm_model
                .attributes
                .iter()
                .map(|a| (a.value.name.clone(), a))
                .collect::<HashMap<String, &ModelAttribute>>();
            let mut deferred_add_columns = HashMap::<&String, &ModelAttribute>::new();

            for attr in &model.attributes {
                let Some(lm_attr) = lm_model_attr_lookup.remove(&attr.value.name) else {
                    // Column is new
                    if attr.foreign_key_reference.is_some() {
                        // Cannot alter the table to add a FK. Rebuild.
                        rebuilds.push((model, lm_model));
                        continue 'models;
                    }

                    deferred_add_columns.insert(&attr.value.name, attr);
                    continue;
                };

                // No diff.
                if attr.hash == lm_attr.hash {
                    continue;
                }

                // Foreign keys cannot be altered in SQLite, defer for a full rebuild.
                if attr.foreign_key_reference != lm_attr.foreign_key_reference {
                    rebuilds.push((model, lm_model));
                    continue 'models;
                }

                // Type mismatch
                // TODO: warnings
                if attr.value.cidl_type != lm_attr.value.cidl_type {
                    // Drop the last migrated column
                    {
                        stmts.push(to_sqlite(
                            Table::alter()
                                .table(alias(&model.name))
                                .drop_column(alias(&lm_attr.value.name))
                                .to_owned(),
                        ));
                    }

                    // Add new
                    {
                        stmts.push(to_sqlite(
                            Table::alter()
                                .table(alias(&model.name))
                                .add_column(typed_column(
                                    &attr.value.name,
                                    &attr.value.cidl_type,
                                    true,
                                ))
                                .to_owned(),
                        ));
                    }

                    continue;
                }

                // Rename
                if attr.value.name != lm_attr.value.name {
                    stmts.push(to_sqlite(
                        Table::alter()
                            .table(alias(&model.name))
                            .rename_column(alias(&lm_attr.value.name), alias(&attr.value.name))
                            .to_owned(),
                    ));
                }
            }

            // `lm_model_attr_lookup` now contains only unreferenced columns, which
            // are to be dropped if not intended for renaming
            for lm_attr in lm_model_attr_lookup.values() {
                let mut alter = Table::alter();
                alter.table(alias(&model.name));

                let rename_options = deferred_add_columns
                    .values()
                    .filter(|ma| ma.value.cidl_type == lm_attr.value.cidl_type)
                    .map(|ma| &ma.value.name)
                    .collect::<Vec<_>>();

                if !rename_options.is_empty() {
                    let solution = intent.ask(MigrationsDilemma::RenameOrDropAttribute {
                        model_name: model.name.clone(),
                        attribute_name: lm_attr.value.name.clone(),
                        options: &rename_options,
                    });

                    if let Some(solution) = solution {
                        let option = &rename_options[solution];
                        alter.rename_column(alias(&lm_attr.value.name), alias(*option));
                        stmts.push(to_sqlite(alter));
                        deferred_add_columns.remove(option);
                        continue;
                    }
                }

                // _not_ a rename, drop.
                alter.drop_column(alias(&lm_attr.value.name));
                stmts.push(to_sqlite(alter));
            }

            // Add the remaining deferred columns
            for attr in deferred_add_columns.values() {
                stmts.push(to_sqlite(
                    Table::alter()
                        .table(alias(&model.name))
                        .add_column(typed_column(&attr.value.name, &attr.value.cidl_type, true))
                        .to_owned(),
                ));
            }

            // Add all alter statements to the result
            res.extend(stmts);
        }

        if !rebuilds.is_empty() {
            res.push("PRAGMA foreign_keys = OFF;".into());
        }

        for (model, lm_model) in &rebuilds {
            // Rename the last migrated model to "name_hash"
            {
                res.push(to_sqlite(
                    Table::rename()
                        .table(
                            alias(&lm_model.name),
                            alias(format!("{}_{}", lm_model.name, lm_model.hash.unwrap())),
                        )
                        .to_owned(),
                ));
            }

            // Create the new model
            {
                let create_stmts = Self::create(vec![model], model_lookup, vec![]);
                for stmt in create_stmts {
                    res.push(stmt);
                }
            }

            // Copy the data from the old table
            {
                let lm_attr_lookup = lm_model
                    .attributes
                    .iter()
                    .map(|a| (&a.value.name, &a.value))
                    .chain(std::iter::once((
                        &lm_model.primary_key.name,
                        &lm_model.primary_key,
                    )))
                    .collect::<HashMap<_, _>>();

                let columns = model
                    .attributes
                    .iter()
                    .map(|a| &a.value)
                    .chain(std::iter::once(&model.primary_key))
                    .collect::<Vec<_>>();

                let insert = Query::insert()
                    .into_table(alias(&model.name))
                    .columns(columns.iter().map(|a| alias(&a.name)))
                    .select_from(
                        Query::select()
                            .from(alias(format!(
                                "{}_{}",
                                lm_model.name,
                                lm_model.hash.unwrap()
                            )))
                            .exprs(columns.iter().map(|model_c| {
                                let Some(lm_c) = lm_attr_lookup.get(&model_c.name) else {
                                    // Column is new, use a default value
                                    return Expr::value(sql_default(&model_c.cidl_type));
                                };

                                let col = Expr::col(alias(&lm_c.name));
                                if lm_c.cidl_type == model_c.cidl_type {
                                    // Column directly transfers to the new table
                                    col.into()
                                } else {
                                    // Column type changed, cast
                                    let sql_type = match &model_c.cidl_type.root_type() {
                                        CidlType::Integer => "integer",
                                        CidlType::Real => "real",
                                        CidlType::Text => "text",
                                        CidlType::Blob => "blob",
                                        _ => unreachable!(),
                                    };

                                    col.cast_as(sql_type)
                                }
                            }))
                            .to_owned(),
                    )
                    .unwrap()
                    .to_owned();

                res.push(format!("{};", insert.to_string(SqliteQueryBuilder)));
            }

            // Drop the old table
            {
                let mut drop = Table::drop();
                drop.table(alias(format!(
                    "{}_{}",
                    lm_model.name,
                    lm_model.hash.unwrap()
                )));

                res.push(to_sqlite(drop));
            }
        }

        if !rebuilds.is_empty() {
            res.push("PRAGMA foreign_keys = ON;".into());
            res.push("PRAGMA foreign_keys_check;".into());
        }

        res
    }

    /// Takes in a vec of last migrated models and deletes all of their m2m tables and tables.
    fn drop(sorted_lm_models: Vec<&Model>) -> Vec<String> {
        let mut res = vec![];

        // Insertion order is dependency before dependent, drop order
        // is dependent before dependency (reverse of insertion)
        for &model in sorted_lm_models.iter().rev() {
            // Drop M2M's
            for m2m_id in model
                .navigation_properties
                .iter()
                .filter_map(|n| match &n.kind {
                    NavigationPropertyKind::ManyToMany { unique_id } => Some(unique_id),
                    _ => None,
                })
            {
                res.push(to_sqlite(
                    Table::drop().table(alias(m2m_id)).if_exists().to_owned(),
                ));
            }

            // Drop table
            res.push(to_sqlite(
                Table::drop()
                    .table(alias(&model.name))
                    .if_exists()
                    .to_owned(),
            ));
        }

        res
    }

    /// Given an AST and the last migrated AST, produces a sequence of SQL queries `CREATE`-ing, `DROP`-ing
    /// and `ALTER`-ing the last migrated AST to sync with the new.
    fn make_migrations(
        ast: &CloesceAst,
        lm_ast: Option<&CloesceAst>,
        mut many_to_many_tables: HashMap<String, JunctionTable>,
        intent: &dyn MigrationsIntent,
    ) -> String {
        let Some(lm_ast) = lm_ast else {
            // No previous migration, insert naively.
            return Self::create(
                ast.models.values().collect(),
                &ast.models,
                many_to_many_tables.into_values().collect(),
            )
            .join("\n");
        };

        let mut sorted_create_models = vec![];
        let mut alter_models = vec![];
        let mut sorted_drop_lms = vec![];

        // Partition altered and newly created models
        for model in ast.models.values() {
            match lm_ast.models.get(&model.name) {
                Some(lm_model) if lm_model.hash != model.hash => {
                    alter_models.push((model, lm_model));
                }
                None => {
                    sorted_create_models.push(model);
                }
                _ => {
                    // No change, skip
                }
            }
        }

        // Partition dropped models
        for lm_model in lm_ast.models.values() {
            if ast.models.get(&lm_model.name).is_none() {
                sorted_drop_lms.push(lm_model);
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

        // It's possible drops were meant to be a rename.
        //
        // TODO: We can do some kind of similarity test between models to discard
        // obvious non-solutions
        if !sorted_drop_lms.is_empty() && !sorted_create_models.is_empty() {
            sorted_drop_lms.retain(|lm_model| {
                let solution = intent.ask(MigrationsDilemma::RenameOrDropModel {
                    model_name: lm_model.name.clone(),
                    options: &sorted_create_models.iter().map(|m| &m.name).collect(),
                });

                let Some(solution) = solution else {
                    return true;
                };

                alter_models.push((sorted_create_models.remove(solution), lm_model));
                false
            });
        }

        let mut res = String::new();
        for (title, stmts) in [
            ("Dropped Models", &Self::drop(sorted_drop_lms)),
            (
                "New Models",
                &Self::create(
                    sorted_create_models,
                    &ast.models,
                    many_to_many_tables.into_values().collect(),
                ),
            ),
            (
                "Altered Models",
                &Self::alter(alter_models, &ast.models, intent),
            ),
        ] {
            if !stmts.is_empty() {
                res.push_str(&format!("--- {title}"));
                res.push('\n');
                res.push_str(&stmts.join("\n"));
                res.push('\n');
            }
        }

        res
    }
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

// TODO: SeaQuery forcing us to do alias everywhere is really annoying,
// it feels like this library is a bad choice for our use case.
fn alias(name: impl Into<String>) -> sea_query::Alias {
    sea_query::Alias::new(name)
}

// TODO: User made default types
fn sql_default(ty: &CidlType) -> sea_query::Value {
    if ty.is_nullable() {
        return sea_query::Value::Int(None);
    }

    match ty {
        CidlType::Integer => sea_query::Value::Int(Some(0i32)),
        CidlType::Text => sea_query::Value::String(Some(Box::new("".into()))),
        CidlType::Real => sea_query::Value::Float(Some(0.0)),
        CidlType::Blob => sea_query::Value::Bytes(Some(Box::new(vec![]))),
        _ => unreachable!(),
    }
}

fn typed_column(name: &str, ty: &CidlType, with_default: bool) -> ColumnDef {
    let mut col = ColumnDef::new(alias(name));
    let inner = match ty {
        CidlType::Nullable(inner) => inner.as_ref(),
        t => t,
    };

    if with_default {
        col.default(sql_default(ty));
    }

    match inner {
        CidlType::Integer => col.integer(),
        CidlType::Real => col.decimal(),
        CidlType::Text => col.text(),
        CidlType::Blob => col.blob(),
        _ => unreachable!("column type must be validated earlier"),
    };
    col
}

fn to_sqlite(builder: impl SchemaStatementBuilder) -> String {
    format!("{};", builder.to_string(SqliteQueryBuilder))
}
