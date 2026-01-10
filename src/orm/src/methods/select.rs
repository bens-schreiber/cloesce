use ast::{Model, NavigationPropertyKind, fail};
use sea_query::{
    Expr, IntoCondition, IntoIden, Query, SelectStatement, SqliteQueryBuilder, TableRef,
};
use serde_json::Value;

use crate::{
    IncludeTreeJson, ModelMeta,
    methods::{OrmErrorKind, alias},
};

use super::Result;

pub struct SelectModel<'a> {
    meta: &'a ModelMeta,
    path: Vec<String>,
    gensym_c: usize,
    query: SelectStatement,
}

impl<'a> SelectModel<'a> {
    pub fn query(
        model_name: &str,
        from: Option<String>,
        include_tree: Option<IncludeTreeJson>,
        meta: &'a ModelMeta,
    ) -> Result<String> {
        let model = match meta.get(model_name) {
            Some(m) => m,
            None => fail!(OrmErrorKind::UnknownModel, "{}", model_name),
        };
        if model.primary_key.is_none() {
            fail!(
                OrmErrorKind::ModelMissingD1,
                "Model '{}' is not a D1 model.",
                model_name
            )
        }

        const CUSTOM_FROM: &str = "__cte_custom_from_placeholder__";
        let mut query = Query::select();
        match from {
            Some(_) => {
                query.from(TableRef::Table(alias(CUSTOM_FROM).into_iden()));
            }
            None => {
                query.from(alias(&model.name));
            }
        }

        let mut sm = Self {
            meta,
            path: vec![],
            gensym_c: 0,
            query,
        };

        let include_tree = include_tree.unwrap_or_default();
        sm.dfs(model, &include_tree, model.name.clone(), None);
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
        tree: &IncludeTreeJson,
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

        let pk = &model.primary_key.as_ref().unwrap().name;

        // Primary Key
        {
            let col = if let Some(m2m_alias) = m2m_alias {
                // M:M pk is "left" or "right", alphabetically sorted.
                // Kind of a hack here but it works.
                let col = if model.name.as_str() < m2m_alias.trim_end_matches("_") {
                    "left"
                } else {
                    "right"
                };
                Expr::col((alias(m2m_alias), alias(col)))
            } else {
                Expr::col((alias(&model_alias), alias(pk)))
            };

            self.query.expr_as(col, alias(join_path(pk)));
        };

        // Columns
        for col in &model.columns {
            self.query.expr_as(
                Expr::col((alias(&model_alias), alias(&col.value.name))),
                alias(join_path(&col.value.name)),
            );
        }

        // Navigation properties
        for nav in &model.navigation_properties {
            let Some(Value::Object(child_tree)) = tree.get(&nav.var_name) else {
                continue;
            };

            let child = self.meta.get(&nav.model_reference).unwrap();
            let child_alias = self.gensym(&child.name);
            let mut child_m2m_alias = None;

            match &nav.kind {
                NavigationPropertyKind::OneToOne { column_reference } => {
                    let nav_model_pk = &child.primary_key.as_ref().unwrap().name;
                    left_join_as(
                        &mut self.query,
                        &child.name,
                        &child_alias,
                        Expr::col((alias(&model_alias), alias(column_reference)))
                            .equals((alias(&child_alias), alias(nav_model_pk))),
                    );
                }
                NavigationPropertyKind::OneToMany { column_reference } => {
                    left_join_as(
                        &mut self.query,
                        &child.name,
                        &child_alias,
                        Expr::col((alias(&model_alias), alias(pk)))
                            .equals((alias(&child_alias), alias(column_reference))),
                    );
                }
                NavigationPropertyKind::ManyToMany => {
                    let nav_model_pk = &child.primary_key;
                    let pk = &model.primary_key.as_ref().unwrap().name;
                    let m2m_table_name = nav.many_to_many_table_name(&model.name);
                    let m2m_alias = self.gensym(&m2m_table_name);

                    let (a, b) = if model.name < nav.model_reference {
                        ("left", "right")
                    } else {
                        ("right", "left")
                    };

                    left_join_as(
                        &mut self.query,
                        &m2m_table_name,
                        &m2m_alias,
                        Expr::col((alias(&model_alias), alias(pk)))
                            .equals((alias(&m2m_alias), alias(a))),
                    );

                    left_join_as(
                        &mut self.query,
                        &child.name,
                        &child_alias,
                        Expr::col((alias(&m2m_alias), alias(b))).equals((
                            alias(&child_alias),
                            alias(&nav_model_pk.as_ref().unwrap().name),
                        )),
                    );

                    child_m2m_alias = Some(m2m_alias);
                }
            }

            self.path.push(nav.var_name.clone());
            self.dfs(child, child_tree, child_alias, child_m2m_alias.as_ref());
            self.path.pop();
        }
    }

    fn gensym(&mut self, name: &str) -> String {
        self.gensym_c += 1;
        format!("{}_{}", name, self.gensym_c)
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

#[cfg(test)]
mod test {
    use ast::{CidlType, NavigationPropertyKind};
    use generator_test::{ModelBuilder, expected_str};
    use serde_json::json;
    use sqlx::{Row, SqlitePool};

    use crate::{
        ModelMeta,
        methods::{select::SelectModel, test_sql},
    };

    #[sqlx::test]
    async fn scalar_model(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Person")
            .id_pk()
            .col("name", CidlType::Text, None)
            .build();

        let meta = vec![ast_model]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let insert_query = r#"
            INSERT INTO Person (id, name) VALUES (1, 'Alice'), (2, 'Bob');
        "#
        .to_string();

        // Act
        let select_stmt =
            SelectModel::query("Person", None, None, &meta).expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Person"."id" AS "id", "Person"."name" AS "name" FROM "Person""#
        );

        let results = test_sql(
            meta,
            vec![(insert_query, vec![]), (select_stmt, vec![])],
            db,
        )
        .await
        .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<String, _>("name").unwrap(), "Alice");
    }

    #[sqlx::test]
    async fn one_to_one(db: SqlitePool) {
        // Arrange
        let meta: ModelMeta = vec![
            ModelBuilder::new("Person")
                .id_pk()
                .col("dogId", CidlType::Integer, Some("Dog".into()))
                .nav_p(
                    "dog",
                    "Dog",
                    NavigationPropertyKind::OneToOne {
                        column_reference: "dogId".into(),
                    },
                )
                .build(),
            ModelBuilder::new("Dog").id_pk().build(),
        ]
        .into_iter()
        .map(|m| (m.name.clone(), m))
        .collect();

        let include_tree = json!({
            "dog": {}
        });

        let insert_query = r#"
            INSERT INTO Dog (id) VALUES (1), (2);
            INSERT INTO Person (id, dogId) VALUES (1, 1), (2, 2);
        "#
        .to_string();

        // Act
        let select_stmt = SelectModel::query(
            "Person",
            None,
            Some(include_tree.as_object().unwrap().clone()),
            &meta,
        )
        .expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Person"."id" AS "id", "Person"."dogId" AS "dogId", "Dog_1"."id" AS "dog.id" FROM "Person" LEFT JOIN "Dog" AS "Dog_1" ON "Person"."dogId" = "Dog_1"."id""#
        );

        let results = test_sql(
            meta,
            vec![(insert_query, vec![]), (select_stmt, vec![])],
            db,
        )
        .await
        .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("dogId").unwrap(), 1);
    }

    #[sqlx::test]
    fn one_to_many(db: SqlitePool) {
        let meta: ModelMeta = vec![
            ModelBuilder::new("Dog")
                .id_pk()
                .col("personId", CidlType::Integer, Some("Person".into()))
                .build(),
            ModelBuilder::new("Cat")
                .col("personId", CidlType::Integer, Some("Person".into()))
                .id_pk()
                .build(),
            ModelBuilder::new("Person")
                .id_pk()
                .nav_p(
                    "dogs",
                    "Dog",
                    NavigationPropertyKind::OneToMany {
                        column_reference: "personId".into(),
                    },
                )
                .nav_p(
                    "cats",
                    "Cat",
                    NavigationPropertyKind::OneToMany {
                        column_reference: "personId".into(),
                    },
                )
                .col("bossId", CidlType::Integer, Some("Boss".into()))
                .build(),
            ModelBuilder::new("Boss")
                .id_pk()
                .nav_p(
                    "persons",
                    "Person",
                    NavigationPropertyKind::OneToMany {
                        column_reference: "bossId".into(),
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

        let insert_query = r#"
            INSERT INTO Boss (id) VALUES (1);
            INSERT INTO Person (id, bossId) VALUES (1, 1), (2, 1);
            INSERT INTO Dog (id, personId) VALUES (1, 1), (2, 2);
            INSERT INTO Cat (id, personId) VALUES (1, 1), (2, 2);
        "#
        .to_string();

        // Act
        let sql = SelectModel::query(
            "Boss",
            None,
            Some(include_tree.as_object().unwrap().clone()),
            &meta,
        )
        .expect("list models to work");

        // Assert
        expected_str!(
            sql,
            r#"
            SELECT 
            "Boss"."id" AS "id", 
            "Person_1"."id" AS "persons.id", 
            "Person_1"."bossId" AS "persons.bossId", 
            "Dog_2"."id" AS "persons.dogs.id", 
            "Dog_2"."personId" AS "persons.dogs.personId", 
            "Cat_3"."id" AS "persons.cats.id", 
            "Cat_3"."personId" AS "persons.cats.personId" 
        FROM "Boss" 
        LEFT JOIN "Person" AS "Person_1" ON "Boss"."id" = "Person_1"."bossId" 
        LEFT JOIN "Dog" AS "Dog_2" ON "Person_1"."id" = "Dog_2"."personId" 
        LEFT JOIN "Cat" AS "Cat_3" ON "Person_1"."id" = "Cat_3"."personId"
        "#
        );

        let results = test_sql(meta, vec![(insert_query, vec![]), (sql, vec![])], db)
            .await
            .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("persons.id").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("persons.bossId").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("persons.dogs.id").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("persons.dogs.personId").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("persons.cats.id").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("persons.cats.personId").unwrap(), 1);
    }

    #[sqlx::test]
    async fn many_to_many(db: SqlitePool) {
        // Arrange
        let meta: ModelMeta = vec![
            ModelBuilder::new("Student")
                .id_pk()
                .nav_p(
                    "courses",
                    "Course".to_string(),
                    NavigationPropertyKind::ManyToMany,
                )
                .build(),
            ModelBuilder::new("Course")
                .id_pk()
                .nav_p(
                    "students",
                    "Student".to_string(),
                    NavigationPropertyKind::ManyToMany,
                )
                .build(),
        ]
        .into_iter()
        .map(|m| (m.name.clone(), m))
        .collect();

        let include_tree = json!({
            "courses": {}
        });

        let insert_query = r#"
            INSERT INTO Student (id) VALUES (1), (2);
            INSERT INTO Course (id) VALUES (1), (2);
            INSERT INTO CourseStudent (left, right) VALUES (1, 1), (1, 2), (2, 1);
        "#
        .to_string();

        // Act
        let select_stmt = SelectModel::query(
            "Student",
            None,
            Some(include_tree.as_object().unwrap().clone()),
            &meta,
        )
        .expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Student"."id" AS "id", "CourseStudent_2"."left" AS "courses.id" FROM "Student" LEFT JOIN "CourseStudent" AS "CourseStudent_2" ON "Student"."id" = "CourseStudent_2"."right" LEFT JOIN "Course" AS "Course_1" ON "CourseStudent_2"."left" = "Course_1"."id""#
        );

        let results = test_sql(
            meta,
            vec![(insert_query, vec![]), (select_stmt, vec![])],
            db,
        )
        .await
        .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("courses.id").unwrap(), 1);
    }

    #[sqlx::test]
    async fn gensym_stops_ambigious_table(db: SqlitePool) {
        // Arrange
        let horse_model = ModelBuilder::new("Horse")
            .id_pk()
            .col("name", CidlType::Text, None)
            .col("bio", CidlType::nullable(CidlType::Text), None)
            .nav_p(
                "matches",
                "Match",
                NavigationPropertyKind::OneToMany {
                    column_reference: "horseId1".into(),
                },
            )
            .build();

        let match_model = ModelBuilder::new("Match")
            .id_pk()
            .col("horseId1", CidlType::Integer, Some("Horse".into()))
            .col("horseId2", CidlType::Integer, Some("Horse".into()))
            .nav_p(
                "horse2",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    column_reference: "horseId2".into(),
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

        let insert_query = r#"
            INSERT INTO Horse (id, name, bio) VALUES (1, 'Spirit', 'Wild and free'), (2, 'Thunder', 'Fast and strong');
            INSERT INTO Match (id, horseId1, horseId2) VALUES (1, 1, 2);
        "#.to_string();

        // Act
        let sql = SelectModel::query(
            "Horse",
            None,
            Some(include_tree.as_object().unwrap().clone()),
            &meta,
        )
        .expect("list models to work");

        // Assert
        expected_str!(
            sql,
            r#"SELECT "Horse"."id" AS "id", "Horse"."name" AS "name", "Horse"."bio" AS "bio", "Match_1"."id" AS "matches.id", "Match_1"."horseId1" AS "matches.horseId1", "Match_1"."horseId2" AS "matches.horseId2", "Horse_2"."id" AS "matches.horse2.id", "Horse_2"."name" AS "matches.horse2.name", "Horse_2"."bio" AS "matches.horse2.bio" FROM "Horse" LEFT JOIN "Match" AS "Match_1" ON "Horse"."id" = "Match_1"."horseId1" LEFT JOIN "Horse" AS "Horse_2" ON "Match_1"."horseId2" = "Horse_2"."id""#
        );

        let results = test_sql(meta, vec![(insert_query, vec![]), (sql, vec![])], db)
            .await
            .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<String, _>("name").unwrap(), "Spirit");
        assert_eq!(value.try_get::<String, _>("bio").unwrap(), "Wild and free");
    }

    #[sqlx::test]
    fn custom_from(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Person")
            .id_pk()
            .col("name", CidlType::Text, None)
            .build();

        let meta = vec![ast_model]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let insert_query = r#"
            INSERT INTO Person (id, name) VALUES (1, 'Alice'), (2, 'Bob');
        "#
        .to_string();

        // Act
        let custom_from = "SELECT * FROM Person WHERE name = 'Alice'".to_string();
        let select_stmt = SelectModel::query("Person", Some(custom_from), None, &meta)
            .expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Person"."id" AS "id", "Person"."name" AS "name" FROM (SELECT * FROM Person WHERE name = 'Alice') AS "Person""#
        );

        let results = test_sql(
            meta,
            vec![(insert_query, vec![]), (select_stmt, vec![])],
            db,
        )
        .await
        .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<String, _>("name").unwrap(), "Alice");
    }
}
