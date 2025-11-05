use std::collections::{HashMap, HashSet};

use ast::{
    CidlType, DataSource, IncludeTree, MigrationsAst, MigrationsModel, ModelAttribute,
    NavigationProperty, NavigationPropertyKind,
};

use indexmap::IndexMap;
use sea_query::{
    ColumnDef, Expr, ForeignKey, Index, IntoCondition, Query, SchemaStatementBuilder,
    SelectStatement, SqliteQueryBuilder, Table,
};

pub struct D1Generator;
impl D1Generator {
    /// Uses the last migrated [MigrationsAst] to produce a new migrated SQL schema.
    ///
    /// Some migration scenarios require user intervention through a [MigrationsIntent], which
    /// can be blocking.
    pub fn migrate(
        ast: &MigrationsAst,
        lm_ast: Option<&MigrationsAst>,
        intent: &dyn MigrationsIntent,
    ) -> String {
        if let Some(lm_ast) = lm_ast
            && lm_ast.hash == ast.hash
        {
            // No work to be done
            return String::default();
        }

        let tables = MigrateTables::make_migrations(ast, lm_ast, intent);
        let views = MigrateViews::make_migrations(ast, lm_ast);
        if lm_ast.is_none() {
            let cloesce_tmp = to_sqlite(
                Table::create()
                    .table(alias("_cloesce_tmp"))
                    .col(
                        ColumnDef::new_with_type(alias("path"), sea_query::ColumnType::Text)
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new_with_type(alias("id"), sea_query::ColumnType::Integer)
                            .not_null(),
                    )
                    .to_owned(),
            );

            return format!("{tables}\n{views}\n--- Cloesce Temporary Table\n{cloesce_tmp}");
        }

        format!("{tables}\n{views}")
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
    fn create(
        models: Vec<(&MigrationsModel, Vec<&DataSource>)>,
        model_lookup: &IndexMap<String, MigrationsModel>,
    ) -> Vec<String> {
        let mut res = vec![];

        for (model, ds) in models {
            for d in ds {
                let mut query = Query::select();
                query.from(alias(&model.name));

                let mut alias_counter = HashMap::<String, u32>::new();
                let model_alias = generate_alias(&model.name, &mut alias_counter);
                dfs(
                    model,
                    &d.tree,
                    &mut query,
                    &mut vec![model.name.clone()],
                    model_alias,
                    None,
                    &mut alias_counter,
                    model_lookup,
                );

                res.push(format!(
                    "CREATE VIEW IF NOT EXISTS \"{}.{}\" AS {};",
                    model.name,
                    d.name,
                    query.to_string(SqliteQueryBuilder)
                ));

                tracing::info!(
                    "Created data source \"{}\" for model \"{}\"",
                    d.name,
                    model.name
                );
            }
        }

        return res;

        #[allow(clippy::too_many_arguments)]
        fn dfs(
            model: &MigrationsModel,
            tree: &IncludeTree,
            query: &mut SelectStatement,
            path: &mut Vec<String>,
            model_alias: String,
            m2m_alias: Option<&String>,
            alias_counter: &mut HashMap<String, u32>,
            model_lookup: &IndexMap<String, MigrationsModel>,
        ) {
            let path_to_column = path.join(".");
            let pk = &model.primary_key.name;

            // Primary Key
            {
                let col = if let Some(m2m_alias) = m2m_alias {
                    // M:M pk is in the form "UniqueIdN.ModelName.PrimaryKeyName"
                    Expr::col((alias(m2m_alias), alias(format!("{}.{}", model.name, pk))))
                } else {
                    Expr::col((alias(&model_alias), alias(pk)))
                };

                query.expr_as(col, alias(format!("{}.{}", &path_to_column, pk)));
            };

            // Columns
            for attr in &model.attributes {
                query.expr_as(
                    Expr::col((alias(&model_alias), alias(&attr.value.name))),
                    alias(format!("{}.{}", &path_to_column, attr.value.name)),
                );
            }

            // Navigation properties
            for nav in &model.navigation_properties {
                let Some(child_tree) = tree.0.get(&nav.var_name) else {
                    continue;
                };

                let child = model_lookup.get(&nav.model_name).unwrap();
                let child_alias = generate_alias(&child.name, alias_counter);
                let mut child_m2m_alias = None;

                match &nav.kind {
                    NavigationPropertyKind::OneToOne { reference } => {
                        let nav_model_pk = &child.primary_key.name;
                        left_join_as(
                            query,
                            &child.name,
                            &child_alias,
                            Expr::col((alias(&model_alias), alias(reference)))
                                .equals((alias(&child_alias), alias(nav_model_pk))),
                        );
                    }
                    NavigationPropertyKind::OneToMany { reference } => {
                        left_join_as(
                            query,
                            &child.name,
                            &child_alias,
                            Expr::col((alias(&model_alias), alias(pk)))
                                .equals((alias(&child_alias), alias(reference))),
                        );
                    }
                    NavigationPropertyKind::ManyToMany { unique_id } => {
                        let nav_model_pk = &child.primary_key;
                        let pk = &model.primary_key.name;
                        let m2m_alias = generate_alias(unique_id, alias_counter);

                        left_join_as(
                            query,
                            unique_id,
                            &m2m_alias,
                            Expr::col((alias(&model_alias), alias(pk))).equals((
                                alias(&m2m_alias),
                                alias(format!("{}.{}", model.name, pk)),
                            )),
                        );

                        left_join_as(
                            query,
                            &child.name,
                            &child_alias,
                            Expr::col((alias(&m2m_alias), alias(format!("{}.{}", child.name, pk))))
                                .equals((alias(&child_alias), alias(&nav_model_pk.name))),
                        );

                        child_m2m_alias = Some(m2m_alias);
                    }
                }

                path.push(nav.var_name.clone());
                dfs(
                    child,
                    child_tree,
                    query,
                    path,
                    child_alias,
                    child_m2m_alias.as_ref(),
                    alias_counter,
                    model_lookup,
                );
                path.pop();
            }
        }

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

    /// If a data source is altered in any way, it will be dropped and added to the create list.
    /// Views don't contain data so I don't see a problem with this (@ben schreiber)
    ///
    /// Returns a list of dropped data source queries, along with a list of data sources that need to created.
    fn drop_alter<'a>(
        ast: &MigrationsAst,
        lm_lookup: &'a IndexMap<String, MigrationsModel>,
    ) -> (Vec<String>, Vec<(&'a MigrationsModel, Vec<&'a DataSource>)>) {
        const DROP_VIEW: &str = "DROP VIEW IF EXISTS";
        let mut drops = vec![];
        let mut creates = HashMap::<&String, Vec<&DataSource>>::new();

        for lm_model in lm_lookup.values() {
            match ast.models.get(&lm_model.name) {
                // Model was changed from last migration
                Some(model) if model.hash != lm_model.hash => {
                    for lm_ds in lm_model.data_sources.values() {
                        let changed = model
                            .data_sources
                            .get(&lm_ds.name)
                            .is_none_or(|ds| ds.hash != lm_ds.hash);

                        // Data Source was changed from last migration
                        if changed {
                            drops
                                .push(format!("{DROP_VIEW} \"{}.{}\";", lm_model.name, lm_ds.name));

                            creates.entry(&lm_model.name).or_default().push(lm_ds);

                            tracing::info!(
                                "Altered data source \"{}\" for model \"{}\"",
                                lm_ds.name,
                                model.name
                            );
                        }
                    }
                }
                // Last migration model was removed entirely
                None => {
                    for lm_ds in lm_model.data_sources.values() {
                        drops.push(format!("{DROP_VIEW} \"{}.{}\";", lm_model.name, lm_ds.name));

                        tracing::info!(
                            "Dropped data source \"{}\" from model \"{}\"",
                            lm_ds.name,
                            lm_model.name
                        );
                    }
                }
                // Last migration model unchanged
                _ => {}
            }
        }

        (
            drops,
            creates
                .drain()
                .map(|(k, v)| (lm_lookup.get(k).unwrap(), v))
                .collect::<Vec<_>>(),
        )
    }

    /// Given a vector of all model trees in the new AST, and the last migrated AST's lookup table,
    /// produces a sequence of SQL queries `CREATE`-ing and `DROP`-ing the last migrated AST to
    /// sync with the new.
    fn make_migrations(ast: &MigrationsAst, lm_ast: Option<&MigrationsAst>) -> String {
        let (drop_stmts, creates) = if let Some(lm_ast) = lm_ast {
            let (drops, mut creates) = Self::drop_alter(ast, &lm_ast.models);

            for model in ast.models.values() {
                if lm_ast.models.contains_key(&model.name) {
                    continue;
                }

                creates.push((model, model.data_sources.values().collect::<Vec<_>>()));
            }

            (drops, creates)
        } else {
            // No last migration: create all
            (
                Vec::default(),
                ast.models
                    .iter()
                    .map(|(_, m)| (m, m.data_sources.values().collect()))
                    .collect::<Vec<_>>(),
            )
        };

        let create_stmts = Self::create(creates, &ast.models);

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

enum AlterKind<'a> {
    RenameTable,
    RebuildTable,

    AddColumn {
        attr: &'a ModelAttribute,
    },
    AlterColumnType {
        attr: &'a ModelAttribute,
        lm_attr: &'a ModelAttribute,
    },
    DropColumn {
        lm_attr: &'a ModelAttribute,
    },

    AddManyToMany {
        unique_id: &'a String,
        model_name: &'a String,
    },
    DropManyToMany {
        unique_id: &'a String,
    },
}

struct MigrateTables;
impl MigrateTables {
    /// Takes in a list of models and junction tables, generating a list
    /// of naive insert queries.
    fn create(
        sorted_models: Vec<&MigrationsModel>,
        model_lookup: &IndexMap<String, MigrationsModel>,
        jcts: HashMap<&String, (&MigrationsModel, &MigrationsModel)>,
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
            tracing::info!("Created table \"{}\"", model.name);
        }

        for (id, (a, b)) in jcts {
            let mut table = Table::create();

            let col_a_name = format!("{}.{}", a.name, a.primary_key.name);
            let mut col_a = typed_column(&col_a_name, &a.primary_key.cidl_type, false);

            let col_b_name = format!("{}.{}", b.name, b.primary_key.name);
            let mut col_b = typed_column(&col_b_name, &b.primary_key.cidl_type, false);

            table
                .table(alias(id))
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
                        .from(alias(id), alias(&col_a_name))
                        .to(alias(&a.name), alias(&a.primary_key.name))
                        .on_update(sea_query::ForeignKeyAction::Cascade)
                        .on_delete(sea_query::ForeignKeyAction::Restrict),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(alias(id), alias(&col_b_name))
                        .to(alias(&b.name), alias(&b.primary_key.name))
                        .on_update(sea_query::ForeignKeyAction::Cascade)
                        .on_delete(sea_query::ForeignKeyAction::Restrict),
                );

            res.push(to_sqlite(table));
            tracing::info!(
                "Created junction table \"{}\" between models \"{}\" \"{}\"",
                id,
                a.name,
                b.name
            );
        }

        res
    }

    /// Generates a sequence of alter statements from a models last migration.
    ///
    /// Some alterations cannot occur in SQLite without dropping the table, in which a
    /// full rebuild and copy of data will occur.
    ///
    /// TODO: Sophisticated warnings and logs about alteration choices
    ///
    /// Poses a [MigrationsDilemma::RenameOrDropModel], determining if a dropped model is
    /// actually just a rename. If that is the case, removes from `drop` and `add` lists, undergoing
    /// table alteration on the (model, last migrated model) pair.
    fn alter<'a>(
        alter_models: Vec<(&'a MigrationsModel, &'a MigrationsModel)>,
        model_lookup: &IndexMap<String, MigrationsModel>,
        intent: &dyn MigrationsIntent,
    ) -> Vec<String> {
        const PRAGMA_FK_OFF: &str = "PRAGMA foreign_keys = OFF;";
        const PRAGMA_FK_ON: &str = "PRAGMA foreign_keys = ON;";
        const PRAGMA_FK_CHECK: &str = "PRAGMA foreign_keys_check;";

        let mut res = vec![];
        let mut visited_m2ms = HashSet::new();

        for (model, lm_model) in alter_models {
            let mut needs_rename_intent = HashMap::<&String, &ModelAttribute>::new();
            let mut needs_drop_intent = vec![];

            for kind in identify_alterations(model, lm_model) {
                match kind {
                    AlterKind::RenameTable => {
                        res.push(to_sqlite(
                            Table::rename()
                                .table(alias(&lm_model.name), alias(&model.name))
                                .to_owned(),
                        ));

                        tracing::info!("Renamed table \"{}\" to \"{}\"", lm_model.name, model.name);
                    }
                    AlterKind::AddColumn { attr } => {
                        needs_rename_intent.insert(&attr.value.name, attr);
                    }
                    AlterKind::AlterColumnType { attr, lm_attr } => {
                        // Drop the last migrated column
                        {
                            res.push(to_sqlite(
                                Table::alter()
                                    .table(alias(&model.name))
                                    .drop_column(alias(&lm_attr.value.name))
                                    .to_owned(),
                            ));
                        }

                        // Add new
                        {
                            res.push(to_sqlite(
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

                        tracing::info!(
                            "Altered column type of \"{}.{:?}\" to {:?}",
                            lm_model.name,
                            lm_attr.value.cidl_type,
                            attr.value.cidl_type
                        );
                        tracing::warn!(
                            "Altering column types drops the previous column. Data can be lost."
                        );
                    }
                    AlterKind::DropColumn { lm_attr } => {
                        needs_drop_intent.push(lm_attr);
                    }
                    AlterKind::AddManyToMany {
                        unique_id,
                        model_name,
                    } => {
                        if !visited_m2ms.insert(unique_id) {
                            continue;
                        }

                        let mut jcts = HashMap::new();

                        let join = model_lookup.get(model_name).unwrap();
                        jcts.insert(
                            unique_id,
                            if model.name > join.name {
                                (model, join)
                            } else {
                                (join, model)
                            },
                        );

                        res.extend(Self::create(vec![], model_lookup, jcts));
                        tracing::warn!(
                            "Created a many to many table \"{}\" between models: \"{}\" \"{}\"",
                            unique_id,
                            model.name,
                            join.name
                        );
                    }
                    AlterKind::DropManyToMany { unique_id } => {
                        if !visited_m2ms.insert(unique_id) {
                            continue;
                        }

                        res.push(to_sqlite(
                            Table::drop().table(alias(unique_id)).if_exists().to_owned(),
                        ));

                        tracing::info!("Dropped a many to many table \"{}\"", unique_id,);
                    }
                    AlterKind::RebuildTable => {
                        let has_fk_col = model
                            .attributes
                            .iter()
                            .any(|a| a.foreign_key_reference.is_some())
                            || lm_model
                                .attributes
                                .iter()
                                .any(|a| a.foreign_key_reference.is_some());
                        if has_fk_col {
                            res.push(PRAGMA_FK_OFF.into());
                        }

                        // Rename the last migrated model to "<name>_<hash>"
                        let name_hash = &format!("{}_{}", lm_model.name, lm_model.hash);
                        {
                            res.push(to_sqlite(
                                Table::rename()
                                    .table(alias(&lm_model.name), alias(name_hash))
                                    .to_owned(),
                            ));
                        }

                        // Create the new model
                        {
                            let create_stmts =
                                Self::create(vec![model], model_lookup, HashMap::default()); // todo: m2ms
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
                                        .from(alias(name_hash))
                                        .exprs(columns.iter().map(|model_c| {
                                            let Some(lm_c) = lm_attr_lookup.get(&model_c.name)
                                            else {
                                                // Column is new, use a default value
                                                return Expr::value(sql_default(
                                                    &model_c.cidl_type,
                                                ));
                                            };

                                            let col = Expr::col(alias(&lm_c.name));
                                            if lm_c.cidl_type == model_c.cidl_type {
                                                // Column directly transfers to the new table
                                                col.into()
                                            } else {
                                                // Column type changed, cast
                                                let sql_type = match &model_c.cidl_type.root_type()
                                                {
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
                            res.push(to_sqlite(Table::drop().table(alias(name_hash)).to_owned()));
                        }

                        if has_fk_col {
                            res.push(PRAGMA_FK_ON.into());
                            res.push(PRAGMA_FK_CHECK.into());
                        }

                        tracing::warn!(
                            "Rebuilt a table \"{}\" by moving data to the new version.",
                            lm_model.name
                        );
                    }
                }
            }

            // Drop or rename columns
            for lm_attr in needs_drop_intent {
                let mut alter = Table::alter();
                alter.table(alias(&model.name));

                let rename_options = needs_rename_intent
                    .values()
                    .filter(|ma| ma.value.cidl_type == lm_attr.value.cidl_type)
                    .map(|ma| &ma.value.name)
                    .collect::<Vec<_>>();

                if !rename_options.is_empty() {
                    let i = intent.ask(MigrationsDilemma::RenameOrDropAttribute {
                        model_name: model.name.clone(),
                        attribute_name: lm_attr.value.name.clone(),
                        options: &rename_options,
                    });

                    // Rename
                    if let Some(i) = i {
                        let option = &rename_options[i];
                        alter.rename_column(alias(&lm_attr.value.name), alias(*option));
                        res.push(to_sqlite(alter));

                        // Remove from the rename pool
                        needs_rename_intent.remove(option);

                        tracing::info!(
                            "Renamed a column \"{}.{}\" to \"{}.{}\"",
                            lm_model.name,
                            lm_attr.value.name,
                            model.name,
                            option
                        );
                        continue;
                    }
                }

                // Drop
                alter.drop_column(alias(&lm_attr.value.name));
                res.push(to_sqlite(alter));
                tracing::info!("Dropped a column \"{}.{}\"", model.name, lm_attr.value.name);
            }

            // Add column
            for add_attr in needs_rename_intent.values() {
                res.push(to_sqlite(
                    Table::alter()
                        .table(alias(&model.name))
                        .add_column(typed_column(
                            &add_attr.value.name,
                            &add_attr.value.cidl_type,
                            true,
                        ))
                        .to_owned(),
                ));
                tracing::info!("Added a column \"{}.{}\"", model.name, add_attr.value.name);
            }
        }

        return res;

        fn identify_alterations<'a>(
            model: &'a MigrationsModel,
            lm_model: &'a MigrationsModel,
        ) -> Vec<AlterKind<'a>> {
            let mut alterations = vec![];

            if model.name != lm_model.name {
                alterations.push(AlterKind::RenameTable);
            }

            if model.primary_key.cidl_type != lm_model.primary_key.cidl_type
                || model.primary_key.name != lm_model.primary_key.name
            {
                return vec![AlterKind::RebuildTable];
            }

            let mut lm_attrs = lm_model
                .attributes
                .iter()
                .map(|a| (&a.value.name, a))
                .collect::<HashMap<&String, &ModelAttribute>>();

            for attr in &model.attributes {
                let Some(lm_attr) = lm_attrs.remove(&attr.value.name) else {
                    if attr.foreign_key_reference.is_some() {
                        return vec![AlterKind::RebuildTable];
                    }

                    alterations.push(AlterKind::AddColumn { attr });
                    continue;
                };

                if lm_attr.hash == attr.hash {
                    continue;
                }

                // Changes on a foreign key column require a rebuild.
                if lm_attr.foreign_key_reference.is_some() || attr.foreign_key_reference.is_some() {
                    return vec![AlterKind::RebuildTable];
                }

                if lm_attr.value.cidl_type != attr.value.cidl_type {
                    alterations.push(AlterKind::AlterColumnType { attr, lm_attr });
                }
            }

            for unvisited_lm_attr in lm_attrs.into_values() {
                if unvisited_lm_attr.foreign_key_reference.is_some() {
                    return vec![AlterKind::RebuildTable];
                }

                alterations.push(AlterKind::DropColumn {
                    lm_attr: unvisited_lm_attr,
                });
            }

            let mut lm_m2ms = lm_model
                .navigation_properties
                .iter()
                .filter_map(|n| match &n.kind {
                    NavigationPropertyKind::ManyToMany { unique_id } => Some((unique_id, n)),
                    _ => None,
                })
                .collect::<HashMap<&String, &NavigationProperty>>();

            for nav in &model.navigation_properties {
                let NavigationPropertyKind::ManyToMany { unique_id } = &nav.kind else {
                    continue;
                };

                if lm_m2ms.remove(unique_id).is_none() {
                    alterations.push(AlterKind::AddManyToMany {
                        unique_id,
                        model_name: &nav.model_name,
                    });
                };
            }

            for unvisited_lm_nav in lm_m2ms.into_values() {
                let NavigationPropertyKind::ManyToMany { unique_id } = &unvisited_lm_nav.kind
                else {
                    unreachable!()
                };
                alterations.push(AlterKind::DropManyToMany { unique_id });
            }

            alterations
        }
    }

    /// Takes in a vec of last migrated models and deletes all of their m2m tables and tables.
    fn drop(sorted_lm_models: Vec<&MigrationsModel>) -> Vec<String> {
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
                tracing::info!("Dropped a many to many table \"{}\"", m2m_id);
            }

            // Drop table
            res.push(to_sqlite(
                Table::drop()
                    .table(alias(&model.name))
                    .if_exists()
                    .to_owned(),
            ));
            tracing::info!("Dropped a table \"{}\"", model.name);
        }

        res
    }

    /// Given an AST and the last migrated AST, produces a sequence of SQL queries `CREATE`-ing, `DROP`-ing
    /// and `ALTER`-ing the last migrated AST to sync with the new.
    fn make_migrations(
        ast: &MigrationsAst,
        lm_ast: Option<&MigrationsAst>,
        intent: &dyn MigrationsIntent,
    ) -> String {
        let _empty = IndexMap::default();
        let lm_models = lm_ast.map(|a| &a.models).unwrap_or(&_empty);

        // Partition all models into three sets, discarding the rest.
        let (creates, create_jcts, alters, drops) = {
            let mut creates = vec![];
            let mut create_m2ms = HashMap::new();
            let mut alters = vec![];
            let mut drops = vec![];

            // Altered and newly created models
            for model in ast.models.values() {
                match lm_models.get(&model.name) {
                    Some(lm_model) if lm_model.hash != model.hash => {
                        alters.push((model, lm_model));
                    }
                    None => {
                        for nav in &model.navigation_properties {
                            let NavigationPropertyKind::ManyToMany { unique_id } = &nav.kind else {
                                continue;
                            };

                            let jct_model = ast.models.get(&nav.model_name).unwrap();
                            create_m2ms.insert(
                                unique_id,
                                if jct_model.name > model.name {
                                    (jct_model, model)
                                } else {
                                    (model, jct_model)
                                },
                            );
                        }

                        creates.push(model);
                    }
                    _ => {
                        // No change, skip
                    }
                }
            }

            // Dropped models
            for lm_model in lm_models.values() {
                if ast.models.get(&lm_model.name).is_none() {
                    drops.push(lm_model);
                    continue;
                }
            }

            // It's possible drops were meant to be a rename.
            //
            // TODO: We can do some kind of similarity test between models to discard
            // obvious non-solutions
            if !drops.is_empty() && !creates.is_empty() {
                drops.retain(|lm_model| {
                    // Blocking input for intentions
                    let solution = intent.ask(MigrationsDilemma::RenameOrDropModel {
                        model_name: lm_model.name.clone(),
                        options: &creates.iter().map(|m| &m.name).collect(),
                    });

                    let Some(solution) = solution else {
                        return true;
                    };

                    alters.push((creates.remove(solution), lm_model));
                    false
                });
            }

            (creates, create_m2ms, alters, drops)
        };

        // Build query
        let mut res = String::new();
        for (title, stmts) in [
            ("Dropped Models", &Self::drop(drops)),
            ("Altered Models", &Self::alter(alters, &ast.models, intent)),
            (
                "New Models",
                &Self::create(creates, &ast.models, create_jcts),
            ),
        ] {
            if stmts.is_empty() {
                continue;
            }

            res.push_str(&format!("--- {title}\n"));
            res.push_str(&stmts.join("\n"));
            res.push('\n');
        }

        res
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
