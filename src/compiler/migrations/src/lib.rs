//! SQLite Migrations generator
//!
//! # Overview
//!
//! This module takes in a [MigrationsIdl], representing the current state of the world, and an optional
//! second [MigrationsIdl] representing the last migrated (abbreviated as "lm") state. From these, it produces
//! a sequence of SQL statements that will migrate a database from the last migrated state to the new state.
//!
//! The main entry point is [MigrationsGenerator::migrate], which produces a new SQL schema from the given IDL pair.
//! If no last migrated IDL is given, it produces a SQL schema from scratch. Otherwise, it identifies the differences
//! between the two IDLs and produces a sequence of SQL statements to alter the last migrated schema into the new schema.
//!
//! ## [MigrationsIntent]
//!
//! Some migration scenarios require user intervention, such as when a model or column is dropped in the new IDL but could
//! potentially be a rename. Because it is impossible to determine the intent from the IDLs alone, the generator poses a
//! [MigrationsDilemma] to a provided [MigrationsIntent], which is a potentially blocking call to allow the user to respond
//! with their intent.

mod fmt;

use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap, HashSet},
};

use idl::{CidlType, Column, MigrationsIdl, MigrationsModel, NavigationField, NavigationFieldKind};

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
        idl: &MigrationsIdl,
        lm_idl: Option<&MigrationsIdl>,
        intent: &dyn MigrationsIntent,
    ) -> String {
        if let Some(lm_ast) = lm_idl
            && lm_ast.hash == idl.hash
        {
            // No work to be done
            return String::default();
        }

        let tables = MigrateTables::make_migrations(idl, lm_idl, intent);
        if lm_idl.is_none() {
            let cloesce_tmp = to_sqlite(
                Table::create()
                    .table(alias("$cloesce_tmp"))
                    .if_not_exists()
                    .col(
                        ColumnDef::new_with_type(alias("path"), sea_query::ColumnType::Text)
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new_with_type(alias("primary_key"), sea_query::ColumnType::Text)
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
        model_name: &'a str,
        options: &'a Vec<&'a str>,
    },
    RenameOrDropColumn {
        model_name: &'a str,
        column_name: &'a str,
        options: &'a Vec<&'a Cow<'a, str>>,
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
        col: &'a Column<'a>,
    },
    AlterColumnType {
        col: &'a Column<'a>,
        lm_col: &'a Column<'a>,
    },
    DropColumn {
        lm_col: &'a Column<'a>,
    },

    AddManyToMany {
        m2m_table_name: String,
        model_name: &'a str,
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
        jcts: HashMap<String, (&MigrationsModel, &MigrationsModel)>,
        model_lookup: &IndexMap<String, MigrationsModel>,
    ) -> Vec<String> {
        let mut res = vec![];

        for model in sorted_models {
            let is_composite_pk = model.primary_columns.len() > 1;
            let mut unique_columns_by_id = BTreeMap::<usize, Vec<&str>>::new();
            let mut fk_groups = BTreeMap::<String, Vec<&Column>>::new();

            let mut table = Table::create();
            table.table(alias(&model.name));
            table.if_not_exists();

            for (col, is_pk) in model.all_columns() {
                // Set primary keys
                if is_pk {
                    let mut column = typed_column(&col.field.name, &col.field.cidl_type, false);
                    if is_composite_pk {
                        column.not_null();
                    } else {
                        column.primary_key();
                    }

                    table.col(column);
                }

                // Set unique indexes
                for unique_id in col.unique_ids.iter() {
                    unique_columns_by_id
                        .entry(*unique_id)
                        .or_default()
                        .push(col.field.name.as_ref());
                }

                // Gather foreign key groups
                let Some(fk_ref) = &col.foreign_key_reference else {
                    continue;
                };

                let ref_model_has_composite_pk = model_lookup
                    .get(fk_ref.model_name)
                    .map(|m| m.primary_columns.len() > 1)
                    .unwrap_or(false);

                let group_key = if let Some(composite_id) = col.composite_id {
                    format!("{}::{}", fk_ref.model_name, composite_id)
                } else if ref_model_has_composite_pk {
                    format!("{}::composite", fk_ref.model_name)
                } else {
                    format!("{}::{}", fk_ref.model_name, col.field.name)
                };

                fk_groups.entry(group_key).or_default().push(col);
            }

            // Composite primary key index
            if is_composite_pk {
                let mut pk = Index::create();
                for pk_col in model.primary_columns.iter() {
                    pk.col(alias(pk_col.field.name.as_ref()));
                }
                table.primary_key(&mut pk);
            }

            // Columns
            for col in model.columns.iter() {
                let mut column = typed_column(&col.field.name, &col.field.cidl_type, false);

                let single_column_unique = col.unique_ids.iter().any(|id| {
                    unique_columns_by_id
                        .get(id)
                        .is_some_and(|cols| cols.len() == 1 && cols[0] == col.field.name)
                });
                if single_column_unique {
                    column.unique_key();
                }

                if !col.field.cidl_type.is_nullable() {
                    column.not_null();
                }

                table.col(column);
            }

            // Foreign keys
            for cols in fk_groups.values_mut() {
                cols.sort_by_key(|c| c.field.name.as_ref());

                let fk_ref = cols
                    .first()
                    .and_then(|c| c.foreign_key_reference.as_ref())
                    .expect("grouped foreign key to have at least one reference");

                let mut fk = ForeignKey::create();
                for col in cols.iter() {
                    fk.from(alias(&model.name), alias(col.field.name.as_ref()));
                    fk.to(
                        alias(fk_ref.model_name),
                        alias(col.foreign_key_reference.as_ref().unwrap().column_name),
                    );
                }

                fk.on_update(sea_query::ForeignKeyAction::Cascade)
                    .on_delete(sea_query::ForeignKeyAction::Restrict);
                table.foreign_key(&mut fk);
            }

            // Multi column unique indexes
            for columns in unique_columns_by_id.values().filter(|cols| cols.len() > 1) {
                let mut index = Index::create();
                index.unique();
                for column_name in columns {
                    index.col(alias(*column_name));
                }
                table.index(&mut index);
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

            let left_join_cols = join_columns_for_side(left, "left");
            let right_join_cols = join_columns_for_side(right, "right");

            table.table(alias(&id)).if_not_exists();

            for (join_col_name, pk) in left_join_cols.iter().chain(right_join_cols.iter()) {
                let mut col = typed_column(join_col_name, &pk.field.cidl_type, false);
                table.col(col.not_null());
            }

            let mut pk_index = Index::create();
            for (join_col_name, _) in left_join_cols.iter().chain(right_join_cols.iter()) {
                pk_index.col(alias(join_col_name.as_str()));
            }
            table.primary_key(&mut pk_index);

            let mut left_fk = ForeignKey::create();
            for (join_col_name, pk) in left_join_cols {
                left_fk
                    .from(alias(id.as_str()), alias(join_col_name))
                    .to(alias(left.name.as_str()), alias(pk.field.name.as_ref()));
            }
            left_fk
                .on_update(sea_query::ForeignKeyAction::Cascade)
                .on_delete(sea_query::ForeignKeyAction::Restrict);
            table.foreign_key(&mut left_fk);

            let mut right_fk = ForeignKey::create();
            for (join_col_name, pk) in right_join_cols {
                right_fk
                    .from(alias(id.as_str()), alias(join_col_name))
                    .to(alias(right.name.as_str()), alias(pk.field.name.as_ref()));
            }
            right_fk
                .on_update(sea_query::ForeignKeyAction::Cascade)
                .on_delete(sea_query::ForeignKeyAction::Restrict);
            table.foreign_key(&mut right_fk);

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
            let mut needs_rename_intent = HashMap::<&str, &Column>::new();
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
                        if is_same_primary_key(model, lm_model) {
                            renamed.insert((&lm_model.name, &model.name));
                        }
                        tracing::info!("Renamed table \"{}\" to \"{}\"", lm_model.name, model.name);
                    }
                    AlterKind::AddColumn { col } => {
                        needs_rename_intent.insert(&col.field.name, col);
                    }
                    AlterKind::AlterColumnType { col, lm_col } => {
                        // Drop the last migrated column
                        {
                            res.push(to_sqlite(
                                Table::alter()
                                    .table(alias(&model.name))
                                    .drop_column(alias(lm_col.field.name.as_ref()))
                                    .to_owned(),
                            ));
                        }

                        // Add new
                        {
                            res.push(to_sqlite(
                                Table::alter()
                                    .table(alias(&model.name))
                                    .add_column(typed_column(
                                        &col.field.name,
                                        &col.field.cidl_type,
                                        true,
                                    ))
                                    .to_owned(),
                            ));
                        }

                        tracing::info!(
                            "Altered column type of \"{}.{:?}\" to {:?}",
                            lm_model.name,
                            lm_col.field.cidl_type,
                            col.field.cidl_type
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

                        res.extend(Self::create(vec![], jcts, model_lookup));
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
                        let has_fk_col = {
                            let m = model
                                .all_columns()
                                .any(|(a, _)| a.foreign_key_reference.is_some());

                            let lm = lm_model
                                .all_columns()
                                .any(|(a, _)| a.foreign_key_reference.is_some());

                            m || lm
                        };

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
                                Self::create(vec![model], HashMap::default(), model_lookup);
                            for stmt in create_stmts {
                                res.push(stmt);
                            }
                        }

                        // Copy the data from the old table
                        {
                            let lm_col_lookup = lm_model
                                .all_columns()
                                .map(|(c, _)| (&c.field.name, &c.field))
                                .collect::<HashMap<_, _>>();

                            let columns = model
                                .all_columns()
                                .map(|(c, _)| &c.field)
                                .collect::<Vec<_>>();

                            let insert = Query::insert()
                                .into_table(alias(&model.name))
                                .columns(columns.iter().map(|a| alias(a.name.as_ref())))
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

                                            let col = Expr::col(alias(lm_c.name.as_ref()));
                                            if lm_c.cidl_type == model_c.cidl_type {
                                                // Column directly transfers to the new table
                                                col.into()
                                            } else {
                                                // Column type changed, cast
                                                let sql_type = match &model_c.cidl_type.root_type()
                                                {
                                                    CidlType::Int | CidlType::Boolean => "integer",
                                                    CidlType::Real => "real",
                                                    CidlType::String | CidlType::DateIso => "text",
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
                    .filter(|ma| ma.field.cidl_type == lm_col.field.cidl_type)
                    .map(|ma| &ma.field.name)
                    .collect::<Vec<_>>();

                if !rename_options.is_empty() {
                    let i = intent.ask(MigrationsDilemma::RenameOrDropColumn {
                        model_name: &model.name,
                        column_name: &lm_col.field.name,
                        options: &rename_options,
                    });

                    // Rename
                    if let Some(i) = i {
                        let option = &rename_options[i];
                        alter.rename_column(
                            alias(lm_col.field.name.as_ref()),
                            alias(option.as_ref()),
                        );
                        res.push(to_sqlite(alter));

                        // Remove from the rename pool
                        needs_rename_intent.remove(option.as_ref());

                        tracing::info!(
                            "Renamed a column \"{}.{}\" to \"{}.{}\"",
                            lm_model.name,
                            lm_col.field.name,
                            model.name,
                            option
                        );
                        continue;
                    }
                }

                // Drop
                alter.drop_column(alias(lm_col.field.name.as_ref()));
                res.push(to_sqlite(alter));
                tracing::info!("Dropped a column \"{}.{}\"", model.name, lm_col.field.name);
            }

            // Add column
            for add_col in needs_rename_intent.values() {
                res.push(to_sqlite(
                    Table::alter()
                        .table(alias(&model.name))
                        .add_column(typed_column(
                            &add_col.field.name,
                            &add_col.field.cidl_type,
                            true,
                        ))
                        .to_owned(),
                ));
                tracing::info!("Added a column \"{}.{}\"", model.name, add_col.field.name);
            }
        }

        return res;

        fn identify_alterations<'a>(
            model: &'a MigrationsModel,
            lm_model: &'a MigrationsModel,
            renamed: &HashSet<(&str, &str)>,
        ) -> Vec<AlterKind<'a>> {
            let mut alterations = vec![];

            if model.name != lm_model.name {
                alterations.push(AlterKind::RenameTable);
            }

            if !is_same_primary_key(model, lm_model) {
                return vec![AlterKind::RebuildTable];
            }

            let mut lm_cols = lm_model
                .columns
                .iter()
                .map(|a| (a.field.name.as_ref(), a))
                .collect::<HashMap<&str, &Column>>();

            for col in &model.columns {
                let Some(lm_col) = lm_cols.remove(col.field.name.as_ref()) else {
                    if col.foreign_key_reference.is_some()
                        || !col.unique_ids.is_empty()
                        || col.composite_id.is_some()
                    {
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
                    && renamed.contains(&(lm_fk_ref.model_name, model_fk_ref.model_name))
                    && lm_fk_ref.column_name == model_fk_ref.column_name
                    && lm_col.field.cidl_type == col.field.cidl_type
                {
                    // If the last migrated column and current column share the same foreign key reference,
                    // and that reference is marked as renamed only, and no type change occurred,
                    // skip because SQLite will have already handled the rename.
                    continue;
                }

                // Changes on a foreign key column require a rebuild.
                if lm_col.foreign_key_reference.is_some()
                    || col.foreign_key_reference.is_some()
                    || lm_col.composite_id != col.composite_id
                {
                    return vec![AlterKind::RebuildTable];
                }

                // Changes on unique constraints require a rebuild.
                if lm_col.unique_ids != col.unique_ids {
                    return vec![AlterKind::RebuildTable];
                }

                if lm_col.field.cidl_type != col.field.cidl_type {
                    alterations.push(AlterKind::AlterColumnType { col, lm_col });
                }
            }

            for unvisited_lm_col in lm_cols.into_values() {
                if unvisited_lm_col.foreign_key_reference.is_some()
                    || !unvisited_lm_col.unique_ids.is_empty()
                    || unvisited_lm_col.composite_id.is_some()
                {
                    return vec![AlterKind::RebuildTable];
                }

                alterations.push(AlterKind::DropColumn {
                    lm_col: unvisited_lm_col,
                });
            }

            let mut lm_m2ms = lm_model
                .navigation_fields
                .iter()
                .filter_map(|n| match &n.kind {
                    NavigationFieldKind::ManyToMany => {
                        Some((n.many_to_many_table_name(&lm_model.name), n))
                    }
                    _ => None,
                })
                .collect::<HashMap<String, &NavigationField>>();

            for nav in &model.navigation_fields {
                let NavigationFieldKind::ManyToMany = &nav.kind else {
                    continue;
                };

                let m2m_table_name = nav.many_to_many_table_name(&model.name);
                if lm_m2ms.remove(&m2m_table_name).is_none() {
                    alterations.push(AlterKind::AddManyToMany {
                        m2m_table_name,
                        model_name: nav.model_reference,
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
                .navigation_fields
                .iter()
                .filter_map(|n| match &n.kind {
                    NavigationFieldKind::ManyToMany => {
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

    /// Given an IDL and the last migrated IDL, produces a sequence of SQL queries `CREATE`-ing, `DROP`-ing
    /// and `ALTER`-ing the last migrated IDL to sync with the new.
    fn make_migrations(
        idl: &MigrationsIdl,
        lm_idl: Option<&MigrationsIdl>,
        intent: &dyn MigrationsIntent,
    ) -> String {
        let _empty = IndexMap::default();
        let lm_models = lm_idl.map(|a| &a.models).unwrap_or(&_empty);

        // Partition all models into three sets, discarding the rest.
        let (creates, create_jcts, alters, drops) = {
            let mut creates = vec![];
            let mut create_m2ms = HashMap::new();
            let mut alters = vec![];
            let mut drops = vec![];

            // Altered and newly created models
            for model in idl.models.values() {
                match lm_models.get(&model.name) {
                    Some(lm_model) if lm_model.hash != model.hash => {
                        alters.push((model, lm_model));
                    }
                    None => {
                        for nav in &model.navigation_fields {
                            let NavigationFieldKind::ManyToMany = &nav.kind else {
                                continue;
                            };

                            let m2m_table_name = nav.many_to_many_table_name(&model.name);
                            let jct_model = idl.models.get(nav.model_reference).unwrap();
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
                if idl.models.get(&lm_model.name).is_none() {
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
                        model_name: lm_model.name.as_ref(),
                        options: &creates.iter().map(|m| m.name.as_str()).collect(),
                    });

                    let Some(solution) = solution else {
                        return true;
                    };

                    // Topological order must be preserved in the alters list.
                    let model = creates.remove(solution);
                    let model_index = idl.models.get_full(&model.name).unwrap().0;
                    let insert_index = alters
                        .iter()
                        .position(|(m, _)| idl.models.get_full(&m.name).unwrap().0 > model_index)
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
                &Self::create(creates, create_jcts, &idl.models),
            ),
            ("Altered Models", &Self::alter(alters, &idl.models, intent)),
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

pub fn alias(name: impl Into<String>) -> sea_query::Alias {
    sea_query::Alias::new(name)
}

fn is_same_primary_key(model: &MigrationsModel, lm_model: &MigrationsModel) -> bool {
    if model.primary_columns.len() != lm_model.primary_columns.len() {
        return false;
    }

    model
        .primary_columns
        .iter()
        .zip(lm_model.primary_columns.iter())
        .all(|(a, b)| {
            a.field.name == b.field.name
                && a.field.cidl_type == b.field.cidl_type
                && a.foreign_key_reference
                    .as_ref()
                    .map(|a| (&a.model_name, &a.column_name))
                    == b.foreign_key_reference
                        .as_ref()
                        .map(|b| (&b.model_name, &b.column_name))
                && a.composite_id == b.composite_id
        })
}

fn join_columns_for_side<'a>(
    model: &'a MigrationsModel,
    side: &'a str,
) -> Vec<(String, &'a Column<'a>)> {
    if model.primary_columns.len() == 1 {
        return vec![(side.into(), &model.primary_columns[0])];
    }

    model
        .primary_columns
        .iter()
        .map(|pk| (format!("{side}_{}", pk.field.name), pk))
        .collect()
}

// TODO: User made default types
fn sql_default(ty: &CidlType) -> sea_query::Value {
    if ty.is_nullable() {
        return sea_query::Value::Int(None);
    }
    match ty {
        CidlType::Int => sea_query::Value::Int(Some(0i32)),
        CidlType::Real => sea_query::Value::Float(Some(0.0)),
        CidlType::String => sea_query::Value::String(Some(Box::new("".into()))),
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
        CidlType::Int | CidlType::Boolean => col.integer(),
        CidlType::Real => col.decimal(),
        CidlType::String | CidlType::DateIso => col.text(),
        CidlType::Blob => col.blob(),
        _ => unreachable!("column type must be validated"),
    };
    col
}

fn to_sqlite(builder: impl SchemaStatementBuilder) -> String {
    format!("{};", builder.to_string(SqliteQueryBuilder))
}
