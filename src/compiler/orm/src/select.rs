use idl::{CloesceIdl, IncludeTree, Model, NavigationFieldKind};
use sea_query::{
    Expr, IntoCondition, IntoIden, Query, SelectStatement, SqliteQueryBuilder, TableRef,
};

use crate::{OrmErrorKind, Result, alias, fail};

pub struct SelectModel<'a> {
    idl: &'a CloesceIdl<'a>,
    path: Vec<String>,
    counter: usize,
    query: SelectStatement,
}

impl<'a> SelectModel<'a> {
    /// Can return errors [OrmErrorKind::UnknownModel] and [OrmErrorKind::ModelMissingD1].
    pub fn query(
        model_name: &str,
        from: Option<String>,
        include_tree: Option<&IncludeTree>,
        idl: &'a CloesceIdl<'a>,
    ) -> Result<String> {
        let model = match idl.models.get(model_name) {
            Some(m) => m,
            None => fail!(OrmErrorKind::UnknownModel {
                name: model_name.to_string(),
            }),
        };
        if !model.uses_sqlite() {
            // Fail silently.
            return Ok(String::default());
        }

        const CUSTOM_FROM: &str = "__custom_from_placeholder__";
        let mut query = Query::select();
        match from {
            Some(_) => {
                query.from(TableRef::Table(alias(CUSTOM_FROM).into_iden()));
            }
            None => {
                query.from(alias(model.name));
            }
        }

        let mut sm = Self {
            idl,
            path: vec![],
            counter: 0,
            query,
        };

        let empty_tree = IncludeTree::default();
        let include_tree = include_tree.unwrap_or(&empty_tree);

        sm.dfs(model, include_tree, model.name.to_string());
        let res = sm.query.to_string(SqliteQueryBuilder);

        // Dumb hack to support custom FROM clauses
        if let Some(custom_from) = from {
            return Ok(res.replace(
                &format!("\"{CUSTOM_FROM}\""),
                &format!("({}) AS \"{}\"", custom_from, model.name),
            ));
        }

        Ok(res)
    }

    fn dfs(&mut self, model: &Model, tree: &IncludeTree, model_alias: String) {
        let join_path = |member: &str| {
            if self.path.is_empty() {
                member.to_string()
            } else {
                format!("{}.{}", self.path.join("."), member)
            }
        };

        // Primary Key columns
        for col in &model.primary_columns {
            let pk_name = &col.field.name;
            self.query.expr_as(
                Expr::col((alias(&model_alias), alias(pk_name.as_ref()))),
                alias(join_path(pk_name.as_ref())),
            );
        }

        // Columns
        for col in &model.columns {
            self.query.expr_as(
                Expr::col((alias(&model_alias), alias(col.field.name.as_ref()))),
                alias(join_path(col.field.name.as_ref())),
            );
        }

        // Navigation fields
        for nav in &model.navigation_fields {
            let Some(child_tree) = tree.0.get(nav.field.name.as_ref()) else {
                continue;
            };

            let child = self.idl.models.get(&nav.model_reference).unwrap();

            if !child.uses_sqlite() {
                // No actual SQL columns to select
                continue;
            }

            let child_alias = self.id(child.name);

            match &nav.kind {
                NavigationFieldKind::OneToOne { fields } => {
                    // Build join condition for all key columns
                    let mut condition = sea_query::Condition::all();

                    for (fk, pk) in fields.iter().zip(child.primary_columns.iter()) {
                        condition = condition.add(
                            Expr::col((alias(&model_alias), alias(*fk)))
                                .equals((alias(&child_alias), alias(pk.field.name.as_ref()))),
                        );
                    }

                    left_join_as(&mut self.query, child.name, &child_alias, condition);
                }
                NavigationFieldKind::OneToMany { columns } => {
                    // Build join condition for all key columns
                    let mut condition = sea_query::Condition::all();

                    for (pk, fk) in model.primary_columns.iter().zip(columns.iter()) {
                        condition = condition.add(
                            Expr::col((alias(&model_alias), alias(pk.field.name.as_ref())))
                                .equals((alias(&child_alias), alias(*fk))),
                        );
                    }

                    left_join_as(&mut self.query, child.name, &child_alias, condition);
                }
            }

            self.path.push(nav.field.name.to_string());
            self.dfs(child, child_tree, child_alias);
            self.path.pop();
        }
    }

    fn id(&mut self, name: &str) -> String {
        self.counter += 1;
        format!("{}_{}", name, self.counter)
    }
}

fn left_join_as(
    query: &mut SelectStatement,
    model_name: &str,
    model_alias: &str,
    condition: impl IntoCondition,
) {
    query.left_join(
        TableRef::Table(alias(model_name).into_iden()).alias(alias(model_alias)),
        condition,
    );
}
