mod fmt;

use std::collections::{HashMap, HashSet};

use ast::{
    CidlType, D1Column, MigrationsAst, MigrationsModel, NavigationProperty, NavigationPropertyKind,
};

use indexmap::IndexMap;
use sea_query::{
    ColumnDef, Expr, ForeignKey, Index, Query, SchemaStatementBuilder, SqliteQueryBuilder, Table,
};

pub struct MigrationsGenerator;
impl MigrationsGenerator {
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
        if lm_ast.is_none() {
            let cloesce_tmp = to_sqlite(
                Table::create()
                    .table(alias("_cloesce_tmp"))
                    .if_not_exists()
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

            return fmt::beautify(format!(
                "{tables}\n--- Cloesce Temporary Table\n{cloesce_tmp}"
            ));
        }

        fmt::beautify(tables.to_string())
    }
}

pub enum MigrationsDilemma<'a> {
    RenameOrDropModel {
        model_name: String,
        options: &'a Vec<&'a String>,
    },
    RenameOrDropColumn {
        model_name: String,
        column_name: String,
        options: &'a Vec<&'a String>,
    },
}

pub trait MigrationsIntent {
    /// A potentially blocking call to await some response to the given [MigrationDilemma]
    ///
    /// Returns None if the model should be dropped, Some if an option presented should be selected.
    fn ask(&self, dilemma: MigrationsDilemma) -> Option<usize>;
}

enum AlterKind<'a> {
    RenameTable,
    RebuildTable,

    AddColumn {
        col: &'a D1Column,
    },
    AlterColumnType {
        col: &'a D1Column,
        lm_col: &'a D1Column,
    },
    DropColumn {
        lm_col: &'a D1Column,
    },

    AddManyToMany {
        m2m_table_name: String,
        model_name: &'a String,
    },
    DropManyToMany {
        m2m_table_name: String,
    },
}

struct MigrateTables;
impl MigrateTables {
    /// Takes in a list of models and junction tables, generating a list
    /// of naive insert queries.
    fn create(
        sorted_models: Vec<&MigrationsModel>,
        model_lookup: &IndexMap<String, MigrationsModel>,
        jcts: HashMap<String, (&MigrationsModel, &MigrationsModel)>,
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

            // Columns
            for col in model.columns.iter() {
                let mut column = typed_column(&col.value.name, &col.value.cidl_type, false);

                if !col.value.cidl_type.is_nullable() {
                    column.not_null();
                }

                // Set column foreign key
                if let Some(fk_model_name) = &col.foreign_key_reference {
                    // Unwrap: safe because `validate_models` and `validate_fks` halt
                    // if the values are missing
                    let pk_name = &model_lookup
                        .get(fk_model_name.as_str())
                        .unwrap()
                        .primary_key
                        .name;

                    table.foreign_key(
                        ForeignKey::create()
                            .from(alias(model.name.clone()), alias(col.value.name.as_str()))
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

        for (id, jct) in jcts {
            let mut table = Table::create();

            let (left, right) = if jct.0.name < jct.1.name {
                (jct.0, jct.1)
            } else {
                (jct.1, jct.0)
            };

            const LEFT_NAME: &str = "left";
            let mut left_col = typed_column(LEFT_NAME, &left.primary_key.cidl_type, false);

            const RIGHT_NAME: &str = "right";
            let mut right_col = typed_column(RIGHT_NAME, &right.primary_key.cidl_type, false);

            table
                .table(alias(&id))
                .if_not_exists()
                .col(left_col.not_null())
                .col(right_col.not_null())
                .primary_key(Index::create().col(alias(LEFT_NAME)).col(alias(RIGHT_NAME)))
                .foreign_key(
                    ForeignKey::create()
                        .from(alias(&id), alias(LEFT_NAME))
                        .to(alias(&left.name), alias(&left.primary_key.name))
                        .on_update(sea_query::ForeignKeyAction::Cascade)
                        .on_delete(sea_query::ForeignKeyAction::Restrict),
                )
                .foreign_key(
                    ForeignKey::create()
                        .from(alias(&id), alias(RIGHT_NAME))
                        .to(alias(&right.name), alias(&right.primary_key.name))
                        .on_update(sea_query::ForeignKeyAction::Cascade)
                        .on_delete(sea_query::ForeignKeyAction::Restrict),
                );

            res.push(to_sqlite(table));
            tracing::info!(
                "Created junction table \"{}\" between models \"{}\" \"{}\"",
                id,
                left.name,
                right.name
            );
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
        alter_models: Vec<(&'a MigrationsModel, &'a MigrationsModel)>,
        model_lookup: &IndexMap<String, MigrationsModel>,
        intent: &dyn MigrationsIntent,
    ) -> Vec<String> {
        const PRAGMA_FK_OFF: &str = "PRAGMA foreign_keys = OFF;";
        const PRAGMA_FK_ON: &str = "PRAGMA foreign_keys = ON;";
        const PRAGMA_FK_CHECK: &str = "PRAGMA foreign_keys_check;";

        let mut res = vec![];
        let mut visited_m2ms = HashSet::new();
        let mut renamed = HashSet::new();

        for (model, lm_model) in alter_models {
            let mut needs_rename_intent = HashMap::<&String, &D1Column>::new();
            let mut needs_drop_intent = vec![];
            let alterations = identify_alterations(model, lm_model, &renamed);

            for kind in alterations {
                match kind {
                    AlterKind::RenameTable => {
                        res.push(to_sqlite(
                            Table::rename()
                                .table(alias(&lm_model.name), alias(&model.name))
                                .to_owned(),
                        ));

                        // Mark the model as renamed, meaning other models do not
                        // need to rebuild if they reference this one.
                        if model.primary_key.name == lm_model.primary_key.name
                            && model.primary_key.cidl_type == lm_model.primary_key.cidl_type
                        {
                            renamed.insert((&lm_model.name, &model.name));
                        }
                        tracing::info!("Renamed table \"{}\" to \"{}\"", lm_model.name, model.name);
                    }
                    AlterKind::AddColumn { col } => {
                        needs_rename_intent.insert(&col.value.name, col);
                    }
                    AlterKind::AlterColumnType { col, lm_col } => {
                        // Drop the last migrated column
                        {
                            res.push(to_sqlite(
                                Table::alter()
                                    .table(alias(&model.name))
                                    .drop_column(alias(&lm_col.value.name))
                                    .to_owned(),
                            ));
                        }

                        // Add new
                        {
                            res.push(to_sqlite(
                                Table::alter()
                                    .table(alias(&model.name))
                                    .add_column(typed_column(
                                        &col.value.name,
                                        &col.value.cidl_type,
                                        true,
                                    ))
                                    .to_owned(),
                            ));
                        }

                        tracing::info!(
                            "Altered column type of \"{}.{:?}\" to {:?}",
                            lm_model.name,
                            lm_col.value.cidl_type,
                            col.value.cidl_type
                        );
                        tracing::warn!(
                            "Altering column types drops the previous column. Data can be lost."
                        );
                    }
                    AlterKind::DropColumn { lm_col } => {
                        needs_drop_intent.push(lm_col);
                    }
                    AlterKind::AddManyToMany {
                        m2m_table_name,
                        model_name,
                    } => {
                        if !visited_m2ms.insert(m2m_table_name.clone()) {
                            continue;
                        }

                        let mut jcts = HashMap::new();

                        let join = model_lookup.get(model_name).unwrap();
                        jcts.insert(m2m_table_name.clone(), (model, join));

                        res.extend(Self::create(vec![], model_lookup, jcts));
                        tracing::warn!(
                            "Created a many to many table \"{}\" between models: \"{}\" \"{}\"",
                            m2m_table_name,
                            model.name,
                            join.name
                        );
                    }
                    AlterKind::DropManyToMany { m2m_table_name } => {
                        if !visited_m2ms.insert(m2m_table_name.clone()) {
                            continue;
                        }

                        res.push(to_sqlite(
                            Table::drop()
                                .table(alias(&m2m_table_name))
                                .if_exists()
                                .to_owned(),
                        ));

                        tracing::info!("Dropped a many to many table \"{}\"", m2m_table_name,);
                    }
                    AlterKind::RebuildTable => {
                        let has_fk_col = model
                            .columns
                            .iter()
                            .any(|a| a.foreign_key_reference.is_some())
                            || lm_model
                                .columns
                                .iter()
                                .any(|a| a.foreign_key_reference.is_some());
                        if has_fk_col {
                            res.push(PRAGMA_FK_OFF.into());
                        }

                        tracing::warn!(
                            "TABLE REBUILD! Rebuilding a table \"{}\" by migrating existing data to a new table schema.",
                            lm_model.name
                        );

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
                                Self::create(vec![model], model_lookup, HashMap::default());
                            for stmt in create_stmts {
                                res.push(stmt);
                            }
                        }

                        // Copy the data from the old table
                        {
                            let lm_col_lookup = lm_model
                                .columns
                                .iter()
                                .map(|a| (&a.value.name, &a.value))
                                .chain(std::iter::once((
                                    &lm_model.primary_key.name,
                                    &lm_model.primary_key,
                                )))
                                .collect::<HashMap<_, _>>();

                            let columns = model
                                .columns
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
                                            let Some(lm_c) = lm_col_lookup.get(&model_c.name)
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
                                                    CidlType::Integer | CidlType::Boolean => {
                                                        "integer"
                                                    }
                                                    CidlType::Real => "real",
                                                    CidlType::Text | CidlType::DateIso => "text",
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
                    }
                }
            }

            // Drop or rename columns
            for lm_col in needs_drop_intent {
                let mut alter = Table::alter();
                alter.table(alias(&model.name));

                let rename_options = needs_rename_intent
                    .values()
                    .filter(|ma| ma.value.cidl_type == lm_col.value.cidl_type)
                    .map(|ma| &ma.value.name)
                    .collect::<Vec<_>>();

                if !rename_options.is_empty() {
                    let i = intent.ask(MigrationsDilemma::RenameOrDropColumn {
                        model_name: model.name.clone(),
                        column_name: lm_col.value.name.clone(),
                        options: &rename_options,
                    });

                    // Rename
                    if let Some(i) = i {
                        let option = &rename_options[i];
                        alter.rename_column(alias(&lm_col.value.name), alias(*option));
                        res.push(to_sqlite(alter));

                        // Remove from the rename pool
                        needs_rename_intent.remove(option);

                        tracing::info!(
                            "Renamed a column \"{}.{}\" to \"{}.{}\"",
                            lm_model.name,
                            lm_col.value.name,
                            model.name,
                            option
                        );
                        continue;
                    }
                }

                // Drop
                alter.drop_column(alias(&lm_col.value.name));
                res.push(to_sqlite(alter));
                tracing::info!("Dropped a column \"{}.{}\"", model.name, lm_col.value.name);
            }

            // Add column
            for add_col in needs_rename_intent.values() {
                res.push(to_sqlite(
                    Table::alter()
                        .table(alias(&model.name))
                        .add_column(typed_column(
                            &add_col.value.name,
                            &add_col.value.cidl_type,
                            true,
                        ))
                        .to_owned(),
                ));
                tracing::info!("Added a column \"{}.{}\"", model.name, add_col.value.name);
            }
        }

        return res;

        fn identify_alterations<'a>(
            model: &'a MigrationsModel,
            lm_model: &'a MigrationsModel,
            renamed: &HashSet<(&String, &String)>,
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

            let mut lm_cols = lm_model
                .columns
                .iter()
                .map(|a| (&a.value.name, a))
                .collect::<HashMap<&String, &D1Column>>();

            for col in &model.columns {
                let Some(lm_col) = lm_cols.remove(&col.value.name) else {
                    if col.foreign_key_reference.is_some() {
                        return vec![AlterKind::RebuildTable];
                    }

                    alterations.push(AlterKind::AddColumn { col });
                    continue;
                };

                if lm_col.hash == col.hash {
                    continue;
                }

                if let (Some(model_fk_ref), Some(lm_fk_ref)) =
                    (&col.foreign_key_reference, &lm_col.foreign_key_reference)
                    && renamed.contains(&(lm_fk_ref, model_fk_ref))
                    && lm_col.value.cidl_type == col.value.cidl_type
                {
                    // If the last migrated column and current column share the same foreign key reference,
                    // and that reference is marked as renamed only, and no type change occurred,
                    // skip because SQLite will have already handled the rename.
                    continue;
                }

                // Changes on a foreign key column require a rebuild.
                if lm_col.foreign_key_reference.is_some() || col.foreign_key_reference.is_some() {
                    return vec![AlterKind::RebuildTable];
                }

                if lm_col.value.cidl_type != col.value.cidl_type {
                    alterations.push(AlterKind::AlterColumnType { col, lm_col });
                }
            }

            for unvisited_lm_col in lm_cols.into_values() {
                if unvisited_lm_col.foreign_key_reference.is_some() {
                    return vec![AlterKind::RebuildTable];
                }

                alterations.push(AlterKind::DropColumn {
                    lm_col: unvisited_lm_col,
                });
            }

            let mut lm_m2ms = lm_model
                .navigation_properties
                .iter()
                .filter_map(|n| match &n.kind {
                    NavigationPropertyKind::ManyToMany => {
                        Some((n.many_to_many_table_name(&lm_model.name), n))
                    }
                    _ => None,
                })
                .collect::<HashMap<String, &NavigationProperty>>();

            for nav in &model.navigation_properties {
                let NavigationPropertyKind::ManyToMany = &nav.kind else {
                    continue;
                };

                let m2m_table_name = nav.many_to_many_table_name(&model.name);
                if lm_m2ms.remove(&m2m_table_name).is_none() {
                    alterations.push(AlterKind::AddManyToMany {
                        m2m_table_name,
                        model_name: &nav.model_reference,
                    });
                };
            }

            for (unvisited_m2m, _) in lm_m2ms.into_iter() {
                alterations.push(AlterKind::DropManyToMany {
                    m2m_table_name: unvisited_m2m,
                });
            }

            alterations
        }
    }

    /// Takes in a vec of last migrated models and deletes all of their m2m tables and tables.
    fn drop(sorted_lm_models: Vec<&MigrationsModel>) -> Vec<String> {
        let mut res = vec![];

        // Insertion order is dependency before dependent, drop order
        // is dependent before dependency (reverse of insertion)
        for &lm_model in sorted_lm_models.iter().rev() {
            // Drop M2M's
            for m2m_id in lm_model
                .navigation_properties
                .iter()
                .filter_map(|n| match &n.kind {
                    NavigationPropertyKind::ManyToMany => {
                        Some(n.many_to_many_table_name(&lm_model.name))
                    }
                    _ => None,
                })
            {
                res.push(to_sqlite(
                    Table::drop().table(alias(&m2m_id)).if_exists().to_owned(),
                ));
                tracing::info!("Dropped a many to many table \"{}\"", m2m_id);
            }

            // Drop table
            res.push(to_sqlite(
                Table::drop()
                    .table(alias(&lm_model.name))
                    .if_exists()
                    .to_owned(),
            ));
            tracing::info!("Dropped a table \"{}\"", lm_model.name);
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
                            let NavigationPropertyKind::ManyToMany = &nav.kind else {
                                continue;
                            };

                            let m2m_table_name = nav.many_to_many_table_name(&model.name);
                            let jct_model = ast.models.get(&nav.model_reference).unwrap();
                            create_m2ms.insert(
                                m2m_table_name,
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

                    // Topological order must be preserved in the alters list.
                    let model = creates.remove(solution);
                    let model_index = ast.models.get_full(&model.name).unwrap().0;
                    let insert_index = alters
                        .iter()
                        .position(|(m, _)| ast.models.get_full(&m.name).unwrap().0 > model_index)
                        .unwrap_or(alters.len());
                    alters.insert(insert_index, (model, lm_model));
                    false
                });
            }

            (creates, create_m2ms, alters, drops)
        };

        // Build query
        let mut res = String::new();
        for (title, stmts) in [
            ("Dropped Models", &Self::drop(drops)),
            (
                "New Models",
                &Self::create(creates, &ast.models, create_jcts),
            ),
            ("Altered Models", &Self::alter(alters, &ast.models, intent)),
        ] {
            if stmts.is_empty() {
                continue;
            }

            res.push_str(&format!("--- {title}\n"));
            res.push_str(&stmts.join("\n"));
        }

        res
    }
}

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
        CidlType::Integer | CidlType::Boolean => col.integer(),
        CidlType::Real => col.decimal(),
        CidlType::Text | CidlType::DateIso => col.text(),
        CidlType::Blob => col.blob(),
        _ => unreachable!("column type must be validated"),
    };
    col
}

fn to_sqlite(builder: impl SchemaStatementBuilder) -> String {
    format!("{};", builder.to_string(SqliteQueryBuilder))
}
