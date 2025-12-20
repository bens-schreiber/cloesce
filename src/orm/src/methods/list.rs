use std::collections::HashMap;

use ast::{D1Model, NavigationPropertyKind, fail};
use sea_query::{
    ColumnRef, CommonTableExpression, Expr, IntoCondition, IntoIden, Query, SelectStatement,
    SqliteQueryBuilder, TableRef, WithClause,
};
use serde_json::Value;

use crate::{
    IncludeTreeJson, ModelMeta,
    methods::{OrmErrorKind, alias},
};

use super::Result;

pub fn list_models(
    model_name: &str,
    include_tree: Option<&IncludeTreeJson>,
    custom_from: Option<String>,
    tag_cte: Option<String>,
    meta: &ModelMeta,
) -> Result<String> {
    let model = match meta.get(model_name) {
        Some(m) => m,
        None => fail!(OrmErrorKind::UnknownModel, "{}", model_name),
    };

    let mut query = Query::select();
    if custom_from.is_some() {
        query.from(TableRef::Table(alias("__CUSTOM_FROM__").into_iden()));
    } else {
        query.from(alias(&model.name));
    }

    let mut alias_counter = HashMap::<String, u32>::new();
    let model_alias = generate_alias(&model.name, &mut alias_counter);
    dfs(
        model,
        include_tree,
        &mut query,
        &mut vec![],
        model_alias,
        None,
        &mut alias_counter,
        meta,
    );

    let view_name = match tag_cte {
        Some(tag) => tag.to_string(),
        None => format!("{}.view", model.name),
    };
    let cte = CommonTableExpression::from_select(query.to_owned())
        .table_name(alias(&view_name))
        .to_owned();

    let select = SelectStatement::new()
        .column(ColumnRef::Asterisk)
        .from(alias(view_name))
        .to_owned();

    let with = WithClause::new().cte(cte).to_owned();

    let res = select.with(with).to_string(SqliteQueryBuilder);

    // Dumb hack to support custom FROM clauses
    if let Some(custom_from) = custom_from {
        return Ok(res.replace(
            "\"__CUSTOM_FROM__\"",
            &format!("({}) AS \"{}\"", custom_from, model.name),
        ));
    }

    Ok(res)
}

#[allow(clippy::too_many_arguments)]
fn dfs(
    model: &D1Model,
    tree: Option<&IncludeTreeJson>,
    query: &mut SelectStatement,
    path: &mut Vec<String>,
    model_alias: String,
    m2m_alias: Option<&String>,
    alias_counter: &mut HashMap<String, u32>,
    meta: &ModelMeta,
) {
    let join_path = |member: &str| {
        if path.is_empty() {
            member.to_string()
        } else {
            format!("{}.{}", path.join("."), member)
        }
    };

    let pk = &model.primary_key.name;

    // Primary Key
    {
        let col = if let Some(m2m_alias) = m2m_alias {
            // M:M pk is in the form "UniqueIdN.ModelName.PrimaryKeyName"
            Expr::col((alias(m2m_alias), alias(format!("{}.{}", model.name, pk))))
        } else {
            Expr::col((alias(&model_alias), alias(pk)))
        };

        query.expr_as(col, alias(join_path(pk)));
    };

    // Columns
    for attr in &model.attributes {
        query.expr_as(
            Expr::col((alias(&model_alias), alias(&attr.value.name))),
            alias(join_path(&attr.value.name)),
        );
    }

    // Navigation properties
    let Some(tree) = tree else {
        return;
    };

    for nav in &model.navigation_properties {
        let Some(Value::Object(child_tree)) = tree.get(&nav.var_name) else {
            continue;
        };

        let child = meta.get(&nav.model_name).unwrap();
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
                    Expr::col((alias(&model_alias), alias(pk)))
                        .equals((alias(&m2m_alias), alias(format!("{}.{}", model.name, pk)))),
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
            Some(child_tree),
            query,
            path,
            child_alias,
            child_m2m_alias.as_ref(),
            alias_counter,
            meta,
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

#[cfg(test)]
mod test {
    use ast::{
        CidlType, NavigationPropertyKind,
        builder::{IncludeTreeBuilder, ModelBuilder},
    };
    use serde_json::json;
    use sqlx::SqlitePool;

    use crate::{ModelMeta, expected_str, methods::test_sql};

    use super::list_models;

    #[sqlx::test]
    async fn scalar_model(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Person")
            .id()
            .attribute("name", CidlType::Text, None)
            .build();

        let meta = vec![ast_model]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        // Act
        let sql = list_models("Person", None, None, None, &meta).expect("list models to work");

        // Assert
        expected_str!(
            sql,
            r#"SELECT "Person"."id" AS "id", "Person"."name" AS "name" FROM "Person""#
        );

        test_sql(meta, vec![(sql, vec![])], db)
            .await
            .expect("SQL to execute");
    }

    #[sqlx::test]
    async fn custom_from(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Person")
            .id()
            .attribute("name", CidlType::Text, None)
            .build();

        let meta = vec![ast_model]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let custom_from = "SELECT * FROM Person ORDER BY name DESC LIMIT 10";

        // Act
        let sql = list_models("Person", None, Some(custom_from.into()), None, &meta)
            .expect("list models to work");

        // Assert
        expected_str!(sql, r#"WITH "Person.view""#);
        expected_str!(
            sql,
            r#"SELECT "Person"."id" AS "id", "Person"."name" AS "name" FROM (SELECT * FROM Person ORDER BY name DESC LIMIT 10) AS "Person""#
        );

        test_sql(meta, vec![(sql, vec![])], db)
            .await
            .expect("SQL to execute");
    }

    #[sqlx::test]
    async fn one_to_one(db: SqlitePool) {
        // Arrange
        let meta: ModelMeta = vec![
            ModelBuilder::new("Person")
                .id()
                .attribute("dogId", CidlType::Integer, Some("Dog".into()))
                .nav_p(
                    "dog",
                    "Dog",
                    NavigationPropertyKind::OneToOne {
                        reference: "dogId".into(),
                    },
                )
                .build(),
            ModelBuilder::new("Dog").id().build(),
        ]
        .into_iter()
        .map(|m| (m.name.clone(), m))
        .collect();

        let include_tree = json!({
            "dog": {}
        });

        // Act
        let sql = list_models(
            "Person",
            Some(&include_tree.as_object().unwrap().clone()),
            None,
            None,
            &meta,
        )
        .expect("list models to work");

        // Assert
        expected_str!(
            sql,
            r#"SELECT "Person"."id" AS "id", "Person"."dogId" AS "dogId", "Dog"."id" AS "dog.id" FROM "Person" LEFT JOIN "Dog" ON "Person"."dogId" = "Dog"."id""#
        );

        test_sql(meta, vec![(sql, vec![])], db)
            .await
            .expect("SQL to execute");
    }

    #[sqlx::test]
    fn one_to_many(db: SqlitePool) {
        let meta: ModelMeta = vec![
            ModelBuilder::new("Dog")
                .id()
                .attribute("personId", CidlType::Integer, Some("Person".into()))
                .build(),
            ModelBuilder::new("Cat")
                .attribute("personId", CidlType::Integer, Some("Person".into()))
                .id()
                .build(),
            ModelBuilder::new("Person")
                .id()
                .nav_p(
                    "dogs",
                    "Dog",
                    NavigationPropertyKind::OneToMany {
                        reference: "personId".into(),
                    },
                )
                .nav_p(
                    "cats",
                    "Cat",
                    NavigationPropertyKind::OneToMany {
                        reference: "personId".into(),
                    },
                )
                .attribute("bossId", CidlType::Integer, Some("Boss".into()))
                .build(),
            ModelBuilder::new("Boss")
                .id()
                .nav_p(
                    "persons",
                    "Person",
                    NavigationPropertyKind::OneToMany {
                        reference: "bossId".into(),
                    },
                )
                .build(),
        ]
        .into_iter()
        .map(|m| (m.name.clone(), m))
        .collect();

        let include_tree = json!({
            "persons": {
                "dogs": {},
                "cats": {}
            }
        });

        // Act
        let sql = list_models(
            "Boss",
            Some(&include_tree.as_object().unwrap().clone()),
            None,
            None,
            &meta,
        )
        .expect("list models to work");

        // Assert
        expected_str!(
            sql,
            r#"SELECT "Boss"."id" AS "id", "Person"."id" AS "persons.id", "Person"."bossId" AS "persons.bossId", "Dog"."id" AS "persons.dogs.id", "Dog"."personId" AS "persons.dogs.personId", "Cat"."id" AS "persons.cats.id", "Cat"."personId" AS "persons.cats.personId" FROM "Boss" LEFT JOIN "Person" ON "Boss"."id" = "Person"."bossId" LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId" LEFT JOIN "Cat" ON "Person"."id" = "Cat"."personId""#
        );

        test_sql(meta, vec![(sql, vec![])], db)
            .await
            .expect("SQL to execute");
    }

    #[sqlx::test]
    async fn many_to_many(db: SqlitePool) {
        let meta: ModelMeta = vec![
            ModelBuilder::new("Student")
                .id()
                .nav_p(
                    "courses",
                    "Course".to_string(),
                    NavigationPropertyKind::ManyToMany {
                        unique_id: "StudentsCourses".into(),
                    },
                )
                .data_source(
                    "withCourses",
                    IncludeTreeBuilder::default().add_node("courses").build(),
                )
                .build(),
            ModelBuilder::new("Course")
                .id()
                .nav_p(
                    "students",
                    "Student".to_string(),
                    NavigationPropertyKind::ManyToMany {
                        unique_id: "StudentsCourses".into(),
                    },
                )
                .build(),
        ]
        .into_iter()
        .map(|m| (m.name.clone(), m))
        .collect();

        let include_tree = json!({
            "courses": {}
        });

        // Act
        let sql = list_models(
            "Student",
            Some(&include_tree.as_object().unwrap().clone()),
            None,
            None,
            &meta,
        )
        .expect("list models to work");

        // Assert
        expected_str!(
            sql,
            r#"SELECT "Student"."id" AS "id", "StudentsCourses"."Course.id" AS "courses.id" FROM "Student" LEFT JOIN "StudentsCourses" ON "Student"."id" = "StudentsCourses"."Student.id" LEFT JOIN "Course" ON "StudentsCourses"."Course.id" = "Course"."id""#
        );

        test_sql(meta, vec![(sql, vec![])], db)
            .await
            .expect("SQL to execute");
    }

    #[sqlx::test]
    async fn views_auto_alias(db: SqlitePool) {
        let horse_model = ModelBuilder::new("Horse")
            .id()
            .attribute("name", CidlType::Text, None)
            .attribute("bio", CidlType::nullable(CidlType::Text), None)
            .nav_p(
                "matches",
                "Match",
                NavigationPropertyKind::OneToMany {
                    reference: "horseId1".into(),
                },
            )
            .build();

        let match_model = ModelBuilder::new("Match")
            .id()
            .attribute("horseId1", CidlType::Integer, Some("Horse".into()))
            .attribute("horseId2", CidlType::Integer, Some("Horse".into()))
            .nav_p(
                "horse2",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    reference: "horseId2".into(),
                },
            )
            .build();

        let meta: ModelMeta = vec![horse_model, match_model]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let include_tree = json!({
            "matches": {
                "horse2": {}
            }
        });

        // Act
        let sql = list_models(
            "Horse",
            Some(&include_tree.as_object().unwrap().clone()),
            None,
            None,
            &meta,
        )
        .expect("list models to work");

        // Assert
        expected_str!(
            sql,
            r#"SELECT "Horse"."id" AS "id", "Horse"."name" AS "name", "Horse"."bio" AS "bio", "Match"."id" AS "matches.id", "Match"."horseId1" AS "matches.horseId1", "Match"."horseId2" AS "matches.horseId2", "Horse1"."id" AS "matches.horse2.id", "Horse1"."name" AS "matches.horse2.name", "Horse1"."bio" AS "matches.horse2.bio" FROM "Horse" LEFT JOIN "Match" ON "Horse"."id" = "Match"."horseId1" LEFT JOIN "Horse" AS "Horse1" ON "Match"."horseId2" = "Horse1"."id""#
        );

        test_sql(meta, vec![(sql, vec![])], db)
            .await
            .expect("SQL to execute");
    }
}
