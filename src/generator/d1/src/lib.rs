mod fmt;

use std::collections::{HashMap, HashSet};

use ast::{
    CidlType, D1ModelAttribute, MigrationsAst, MigrationsModel, NavigationProperty,
    NavigationPropertyKind,
};

use indexmap::IndexMap;
use sea_query::{
    ColumnDef, Expr, ForeignKey, Index, Query, SchemaStatementBuilder, SqliteQueryBuilder, Table,
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

enum AlterKind<'a> {
    RenameTable,
    RebuildTable,

    AddColumn {
        attr: &'a D1ModelAttribute,
    },
    AlterColumnType {
        attr: &'a D1ModelAttribute,
        lm_attr: &'a D1ModelAttribute,
    },
    DropColumn {
        lm_attr: &'a D1ModelAttribute,
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
            let mut needs_rename_intent = HashMap::<&String, &D1ModelAttribute>::new();
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
                .collect::<HashMap<&String, &D1ModelAttribute>>();

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
        let lm_models = lm_ast.map(|a| &a.d1_models).unwrap_or(&_empty);

        // Partition all models into three sets, discarding the rest.
        let (creates, create_jcts, alters, drops) = {
            let mut creates = vec![];
            let mut create_m2ms = HashMap::new();
            let mut alters = vec![];
            let mut drops = vec![];

            // Altered and newly created models
            for model in ast.d1_models.values() {
                match lm_models.get(&model.name) {
                    Some(lm_model) if lm_model.hash != model.hash => {
                        alters.push((model, lm_model));
                    }
                    None => {
                        for nav in &model.navigation_properties {
                            let NavigationPropertyKind::ManyToMany { unique_id } = &nav.kind else {
                                continue;
                            };

                            let jct_model = ast.d1_models.get(&nav.model_name).unwrap();
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
                if ast.d1_models.get(&lm_model.name).is_none() {
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
            (
                "Altered Models",
                &Self::alter(alters, &ast.d1_models, intent),
            ),
            (
                "New Models",
                &Self::create(creates, &ast.d1_models, create_jcts),
            ),
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
