use ast::{CloesceAst, IncludeTree, Model, NavigationFieldKind};
use sea_query::{
    Expr, IntoCondition, IntoIden, Query, SelectStatement, SqliteQueryBuilder, TableRef,
};

use crate::{OrmErrorKind, Result, alias, fail};

pub struct SelectModel<'a> {
    ast: &'a CloesceAst<'a>,
    path: Vec<String>,
    counter: usize,
    query: SelectStatement,
}

impl<'a> SelectModel<'a> {
    /// Can return errors [OrmErrorKind::UnknownModel] and [OrmErrorKind::ModelMissingD1].
    pub fn query(
        model_name: &str,
        from: Option<String>,
        include_tree: Option<IncludeTree>,
        ast: &'a CloesceAst<'a>,
    ) -> Result<String> {
        let model = match ast.models.get(model_name) {
            Some(m) => m,
            None => fail!(OrmErrorKind::UnknownModel {
                name: model_name.to_string(),
            }),
        };
        if model.primary_columns.is_empty() {
            fail!(OrmErrorKind::ModelMissingD1 {
                name: model_name.to_string(),
            })
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
            ast,
            path: vec![],
            counter: 0,
            query,
        };

        let include_tree = include_tree.unwrap_or_default();
        sm.dfs(model, &include_tree, model.name.to_string(), None);
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

    fn dfs(
        &mut self,
        model: &Model,
        tree: &IncludeTree,
        model_alias: String,
        m2m_alias: Option<&String>,
    ) {
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

            let col = if let Some(m2m_alias) = m2m_alias {
                // For M:M tables with composite PKs:
                // - single PK: use "left" or "right" (alphabetically sorted)
                // - composite PK: use "left_<pk_name>" or "right_<pk_name>"
                let base = if model.name < m2m_alias.trim_end_matches("_") {
                    "left"
                } else {
                    "right"
                };

                let col_name = if model.primary_columns.len() == 1 {
                    base.to_string()
                } else {
                    format!("{}_{}", base, pk_name)
                };

                Expr::col((alias(m2m_alias), alias(&col_name)))
            } else {
                Expr::col((alias(&model_alias), alias(pk_name.as_ref())))
            };

            self.query.expr_as(col, alias(join_path(pk_name.as_ref())));
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

            let child = self.ast.models.get(&nav.model_reference).unwrap();
            let child_alias = self.id(child.name);
            let mut child_m2m_alias = None;

            match &nav.kind {
                NavigationFieldKind::OneToOne { columns } => {
                    // Build join condition for all key columns
                    let mut condition = sea_query::Condition::all();

                    for (fk, pk) in columns.iter().zip(child.primary_columns.iter()) {
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
                NavigationFieldKind::ManyToMany => {
                    let m2m_table_name = nav.many_to_many_table_name(model.name);
                    let m2m_alias = self.id(&m2m_table_name);

                    let (side_a, side_b) = if model.name < nav.model_reference {
                        ("left", "right")
                    } else {
                        ("right", "left")
                    };

                    // Join from current model to M:M table
                    // Handle both single and composite primary keys
                    let mut condition_a = sea_query::Condition::all();
                    for pk in &model.primary_columns {
                        let m2m_col = if model.primary_columns.len() == 1 {
                            side_a.to_string()
                        } else {
                            format!("{}_{}", side_a, pk.field.name)
                        };

                        condition_a = condition_a.add(
                            Expr::col((alias(&model_alias), alias(pk.field.name.as_ref())))
                                .equals((alias(&m2m_alias), alias(&m2m_col))),
                        );
                    }

                    left_join_as(&mut self.query, &m2m_table_name, &m2m_alias, condition_a);

                    // Join from M:M table to child model
                    // Handle both single and composite primary keys
                    let mut condition_b = sea_query::Condition::all();
                    for pk in &child.primary_columns {
                        let m2m_col = if child.primary_columns.len() == 1 {
                            side_b.to_string()
                        } else {
                            format!("{}_{}", side_b, pk.field.name)
                        };

                        condition_b = condition_b.add(
                            Expr::col((alias(&m2m_alias), alias(&m2m_col)))
                                .equals((alias(&child_alias), alias(pk.field.name.as_ref()))),
                        );
                    }

                    left_join_as(&mut self.query, child.name, &child_alias, condition_b);

                    child_m2m_alias = Some(m2m_alias);
                }
            }

            self.path.push(nav.field.name.to_string());
            self.dfs(child, child_tree, child_alias, child_m2m_alias.as_ref());
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
