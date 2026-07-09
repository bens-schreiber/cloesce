use idl::{CloesceIdl, IncludeTree, Model};
use sea_query::{
    Expr, IntoCondition, IntoIden, Query, SelectStatement, SqliteQueryBuilder, TableRef,
};

use crate::alias;

pub struct SelectModel<'a> {
    idl: &'a CloesceIdl<'a>,
    path: Vec<String>,
    counter: usize,
    query: SelectStatement,
}

impl<'a> SelectModel<'a> {
    pub fn query(
        model_name: &str,
        from: Option<String>,
        include_tree: Option<&IncludeTree>,
        idl: &'a CloesceIdl<'a>,
    ) -> String {
        let model = match idl.models.get(model_name) {
            Some(m) => m,
            None => return String::default(), // Fail silently if the model is not found
        };
        if !model.uses_sqlite() {
            // Fail silently.
            return String::default();
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
            return res.replace(
                &format!("\"{CUSTOM_FROM}\""),
                &format!("({}) AS \"{}\"", custom_from, model.name),
            );
        }

        res
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

            // Join on each resolved discriminator pair: `self.local = target.target`.
            let mut condition = sea_query::Condition::all();
            for key in &nav.keys {
                condition = condition.add(
                    Expr::col((alias(&model_alias), alias(key.local)))
                        .equals((alias(&child_alias), alias(key.target))),
                );
            }

            left_join_as(&mut self.query, child.name, &child_alias, condition);

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
