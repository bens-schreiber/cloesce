use ast::{CidlType, Model, NavigationPropertyKind};
use sea_query::{ColumnRef, Expr, Func, IntoIden, Query, SimpleExpr, SqliteQueryBuilder, TableRef};
use serde_json::Value;

use super::{Result, alias};
use crate::{IncludeTreeJson, ModelMeta, fail, methods::OrmErrorKind};

pub fn select_as_json(
    model_name: &str,
    include_tree: Option<IncludeTreeJson>,
    meta: &ModelMeta,
) -> Result<String> {
    let model = match meta.get(model_name) {
        Some(m) => m,
        None => fail!(OrmErrorKind::UnknownModel, "{}", model_name),
    };

    let include_tree = include_tree.unwrap_or_default();
    let expr = dfs(model, &include_tree, meta)?.build_as_array();

    // Hack to get just the SimpleExpr SQL string
    let as_json_query = Query::select()
        .expr(expr)
        .to_string(SqliteQueryBuilder)
        .trim_start_matches("SELECT ")
        .trim_end_matches(";")
        .to_string();

    Ok(as_json_query)
}

fn dfs(
    model: &Model,
    include_tree: &IncludeTreeJson,
    meta: &ModelMeta,
) -> Result<JsonQueryBuilder> {
    let Some(pk) = &model.primary_key else {
        fail!(
            OrmErrorKind::ModelMissingD1,
            "Model '{}' is not a D1 model.",
            model.name
        )
    };

    let mut builder = JsonQueryBuilder::default();

    // Primary key
    builder.scalar(
        &pk.name,
        ColumnRef::TableColumn(alias(&model.name).into_iden(), alias(&pk.name).into_iden()),
        pk.cidl_type.clone(),
    );

    // Columns
    for column in &model.columns {
        builder.scalar(
            &column.value.name,
            ColumnRef::TableColumn(
                alias(&model.name).into_iden(),
                alias(&column.value.name).into_iden(),
            ),
            column.value.cidl_type.clone(),
        );
    }

    // Navigation properties
    for nav in &model.navigation_properties {
        let Some(Value::Object(sub_tree)) = include_tree.get(&nav.var_name) else {
            continue;
        };

        let related_model = meta.get(&nav.model_reference).unwrap();
        let expr = dfs(related_model, sub_tree, meta)?;
        let from = TableRef::Table(alias(&related_model.name).into_iden());
        let parent_alias = alias(&model.name).into_iden();
        let related_alias = alias(&related_model.name).into_iden();

        match &nav.kind {
            NavigationPropertyKind::OneToOne { column_reference } => {
                let where_clause = Expr::col(ColumnRef::TableColumn(
                    related_alias.clone(),
                    alias(&related_model.primary_key.as_ref().unwrap().name).into_iden(),
                ))
                .eq(Expr::col(ColumnRef::TableColumn(
                    parent_alias.clone(),
                    alias(column_reference).into_iden(),
                )));

                builder.object(&nav.var_name, from, where_clause, expr.build());
            }
            NavigationPropertyKind::OneToMany { column_reference } => {
                let where_clause = Expr::col(ColumnRef::TableColumn(
                    related_alias.clone(),
                    alias(column_reference).into_iden(),
                ))
                .eq(Expr::col(ColumnRef::TableColumn(
                    parent_alias.clone(),
                    alias(&pk.name).into_iden(),
                )));

                builder.array(&nav.var_name, from, where_clause, expr.build());
            }
            NavigationPropertyKind::ManyToMany => {
                let join_alias = alias(nav.many_to_many_table_name(&model.name)).into_iden();
                let (a, b) = if model.name < related_model.name {
                    ("right", "left")
                } else {
                    ("left", "right")
                };
                let join_col_related = alias(a).into_iden();
                let join_col_parent = alias(b).into_iden();

                // JOIN <join_table> ON <join_table>."Related.id" = Related.id
                let join_on = Expr::col(ColumnRef::TableColumn(
                    join_alias.clone(),
                    join_col_related.clone(),
                ))
                .eq(Expr::col(ColumnRef::TableColumn(
                    related_alias.clone(),
                    alias(&related_model.primary_key.as_ref().unwrap().name).into_iden(),
                )));

                // WHERE <join_table>."Parent.id" = Parent.id
                let where_clause = Expr::col(ColumnRef::TableColumn(
                    join_alias.clone(),
                    join_col_parent.clone(),
                ))
                .eq(Expr::col(ColumnRef::TableColumn(
                    parent_alias.clone(),
                    alias(&pk.name).into_iden(),
                )));

                builder.array_with_join(
                    &nav.var_name,
                    from,
                    TableRef::Table(join_alias),
                    join_on,
                    where_clause,
                    expr.build(),
                );
            }
        }
    }

    Ok(builder)
}
struct JsonQueryObject {
    var_name: String,
    from: TableRef,
    joins: Vec<(TableRef, SimpleExpr)>,
    where_clause: SimpleExpr,
    inner: SimpleExpr,
}

#[derive(Default)]
struct JsonQueryBuilder {
    pub scalars: Vec<(String, ColumnRef, CidlType)>,
    pub objects: Vec<JsonQueryObject>,
    pub arrays: Vec<JsonQueryObject>,
}

impl JsonQueryBuilder {
    pub fn scalar(&mut self, column_name: &str, column_ref: ColumnRef, cidl_type: CidlType) {
        self.scalars
            .push((column_name.to_string(), column_ref, cidl_type));
    }

    pub fn object(
        &mut self,
        var_name: &str,
        from: TableRef,
        where_clause: SimpleExpr,
        inner: SimpleExpr,
    ) {
        self.objects.push(JsonQueryObject {
            var_name: var_name.to_string(),
            from,
            where_clause,
            inner,
            joins: vec![],
        });
    }

    pub fn array(
        &mut self,
        var_name: &str,
        from: TableRef,
        where_clause: SimpleExpr,
        inner: SimpleExpr,
    ) {
        self.arrays.push(JsonQueryObject {
            var_name: var_name.to_string(),
            from,
            where_clause,
            inner,
            joins: vec![],
        });
    }

    pub fn array_with_join(
        &mut self,
        var_name: &str,
        from: TableRef,
        join_table: TableRef,
        join_on: SimpleExpr,
        where_clause: SimpleExpr,
        inner: SimpleExpr,
    ) {
        self.arrays.push(JsonQueryObject {
            var_name: var_name.to_string(),
            from,
            joins: vec![(join_table, join_on)],
            where_clause,
            inner,
        });
    }

    pub fn build(self) -> SimpleExpr {
        let mut parts: Vec<SimpleExpr> = Vec::new();
        for (name, column_ref, cidl_type) in self.scalars {
            let column_expr: SimpleExpr = match cidl_type.root_type() {
                // if the cidl type is b64 it must be converted to hex for json serialization
                CidlType::Blob => Func::cust("hex").arg(Expr::col(column_ref)).into(),
                _ => Expr::col(column_ref).into(),
            };

            parts.push(SimpleExpr::Value(name.into()));
            parts.push(column_expr);
        }

        for object in self.objects {
            parts.push(SimpleExpr::Value(object.var_name.into()));

            let subquery = sea_query::Query::select()
                .expr(object.inner)
                .from(object.from)
                .and_where(object.where_clause)
                .to_owned();

            let coalesce_expr = Func::coalesce([
                SimpleExpr::SubQuery(
                    None,
                    Box::new(sea_query::SubQueryStatement::SelectStatement(subquery)),
                ),
                Expr::val("{}").into(),
            ]);

            // => coalesce ( ( select json_object(<expr>) from <table> where <where>  ), '{}' )
            parts.push(coalesce_expr.into());
        }

        for array in self.arrays {
            parts.push(SimpleExpr::Value(array.var_name.into()));

            let json_query = Func::cust("json_group_array").arg(array.inner);
            let mut sub = sea_query::Query::select();
            sub.expr(json_query).from(array.from);
            for (join_table, join_on) in array.joins {
                sub.join(sea_query::JoinType::InnerJoin, join_table, join_on);
            }

            sub.and_where(array.where_clause);
            let coalesce_expr = Func::coalesce([
                SimpleExpr::SubQuery(
                    None,
                    Box::new(sea_query::SubQueryStatement::SelectStatement(
                        sub.to_owned(),
                    )),
                ),
                Expr::val("[]").into(),
            ]);
            parts.push(coalesce_expr.into());
        }

        Func::cust("json_object").args(parts).into()
    }

    pub fn build_as_array(self) -> SimpleExpr {
        let json_query = Func::cust("json_group_array").arg(self.build());
        json_query.into()
    }
}

#[cfg(test)]
mod test {
    use ast::{CidlType, NavigationPropertyKind};
    use generator_test::{ModelBuilder, expected_str};
    use serde_json::json;
    use sqlx::{Row, sqlite::SqlitePool};

    use crate::methods::{json::select_as_json, test_sql};

    #[sqlx::test]
    fn scalar_model(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Person")
            .id_pk()
            .col("name", CidlType::Text, None)
            .col("blob", CidlType::nullable(CidlType::Blob), None)
            .col(
                "favoriteRealNumber",
                CidlType::nullable(CidlType::Real),
                None,
            )
            .build();

        let meta = vec![ast_model]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let include_tree = None;

        let insert_query = r#"
            INSERT INTO Person (id, name, blob, favoriteRealNumber) VALUES (1, 'Alice', X'48656C6C6F', 42.0), (2, 'Bob', NULL, NULL);
        "#;

        // Act
        let expr = select_as_json("Person", include_tree, &meta).expect("as_json to work");

        // Assert
        let results = test_sql(
            meta,
            vec![
                (insert_query.to_string(), vec![]),
                (format!("SELECT {} FROM Person;", expr), vec![]),
            ],
            db,
        )
        .await
        .expect("Upsert to work");

        let expected = r#"[{"id":1,"name":"Alice","blob":"48656C6C6F","favoriteRealNumber": 42.0},{"id":2,"name":"Bob","blob":"","favoriteRealNumber":null}]"#;
        let value: String = results[1][0].try_get(0).unwrap();
        expected_str!(value, expected);
    }

    #[sqlx::test]
    fn one_to_one(db: SqlitePool) {
        // Arrange
        let person_model = ModelBuilder::new("Person")
            .id_pk()
            .col("name", CidlType::Text, None)
            .col("profile_id", CidlType::Integer, Some("Profile".to_string()))
            .nav_p(
                "profile",
                "Profile",
                NavigationPropertyKind::OneToOne {
                    column_reference: "profile_id".to_string(),
                },
            )
            .build();

        let profile_model = ModelBuilder::new("Profile")
            .id_pk()
            .col("bio", CidlType::Text, None)
            .build();

        let meta = vec![person_model, profile_model]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let include_tree = Some(
            json!({
                "profile": {
                }
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let insert_query = r#"
            INSERT INTO Profile (id, bio) VALUES (1, 'Bio of Alice'), (2, 'Bio of Bob');
            INSERT INTO Person (id, name, profile_id) VALUES (1, 'Alice', 1), (2, 'Bob', 2);
        "#;

        // Act
        let expr = select_as_json("Person", include_tree, &meta).expect("as_json to work");

        // Assert
        let results = test_sql(
            meta,
            vec![
                (insert_query.to_string(), vec![]),
                (format!("SELECT {} FROM Person;", expr), vec![]),
            ],
            db,
        )
        .await
        .expect("Upsert to work");

        let expected = r#"[{"id":1,"name":"Alice","profile_id":1,"profile":{"id":1,"bio":"Bio of Alice"}},{"id":2,"name":"Bob","profile_id":2,"profile":{"id":2,"bio":"Bio of Bob"}}]"#;
        let value: String = results[1][0].try_get(0).unwrap();
        expected_str!(value, expected);
    }

    #[sqlx::test]
    fn one_to_many(db: SqlitePool) {
        // Arrange
        let author_model = ModelBuilder::new("Author")
            .id_pk()
            .col("name", CidlType::Text, None)
            .nav_p(
                "books",
                "Book",
                NavigationPropertyKind::OneToMany {
                    column_reference: "author_id".to_string(),
                },
            )
            .build();

        let book_model = ModelBuilder::new("Book")
            .id_pk()
            .col("title", CidlType::Text, None)
            .col("author_id", CidlType::Integer, Some("Author".to_string()))
            .build();

        let meta = vec![author_model, book_model]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let include_tree = Some(
            json!({
                "books": {
                }
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let insert_query = r#"
            INSERT INTO Author (id, name) VALUES (1, 'Author A'), (2, 'Author B');
            INSERT INTO Book (id, title, author_id) VALUES (1, 'Book 1', 1), (2, 'Book 2', 1), (3, 'Book 3', 2);
        "#;

        // Act
        let expr = select_as_json("Author", include_tree, &meta).expect("as_json to work");

        // Assert
        let results = test_sql(
            meta,
            vec![
                (insert_query.to_string(), vec![]),
                (format!("SELECT {} FROM Author;", expr), vec![]),
            ],
            db,
        )
        .await
        .expect("Upsert to work");

        let expected = r#"[{"id":1,"name":"Author A","books":[{"id":1,"title":"Book 1","author_id":1},{"id":2,"title":"Book 2","author_id":1}]},{"id":2,"name":"Author B","books":[{"id":3,"title":"Book 3","author_id":2}]}]"#;
        let value: String = results[1][0].try_get(0).unwrap();
        expected_str!(value, expected);
    }

    #[sqlx::test]
    fn many_to_many(db: SqlitePool) {
        // Arrange
        let student_model = ModelBuilder::new("Student")
            .id_pk()
            .col("name", CidlType::Text, None)
            .nav_p("courses", "Course", NavigationPropertyKind::ManyToMany)
            .build();

        let course_model = ModelBuilder::new("Course")
            .id_pk()
            .col("title", CidlType::Text, None)
            .nav_p("students", "Student", NavigationPropertyKind::ManyToMany)
            .build();

        let meta = vec![student_model, course_model]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let include_tree = Some(
            json!({
                "courses": {
                }
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let insert_query = r#"
            INSERT INTO Student (id, name) VALUES (1, 'Student A'), (2, 'Student B');
            INSERT INTO Course (id, title) VALUES (1, 'Course 1'), (2, 'Course 2');
            INSERT INTO CourseStudent ("left", "right") VALUES (1, 1), (2, 1), (2, 2);
        "#;

        // Act
        let expr = select_as_json("Student", include_tree, &meta).expect("as_json to work");

        // Assert
        let results = test_sql(
            meta,
            vec![
                (insert_query.to_string(), vec![]),
                (format!("SELECT {} FROM Student;", expr), vec![]),
            ],
            db,
        )
        .await
        .expect("Upsert to work");

        let expected = r#"[{"id":1,"name":"Student A","courses":[{"id":1,"title":"Course 1"},{"id":2,"title":"Course 2"}]},{"id":2,"name":"Student B","courses":[{"id":2,"title":"Course 2"}]}]"#;
        let value: String = results[1][0].try_get(0).unwrap();
        expected_str!(value, expected);
    }

    #[sqlx::test]
    fn complex_nesting(db: SqlitePool) {
        // Arrange
        let user_model = ModelBuilder::new("User")
            .id_pk()
            .col("username", CidlType::Text, None)
            .col("profile_id", CidlType::Integer, Some("Profile".to_string()))
            .nav_p(
                "profile",
                "Profile",
                NavigationPropertyKind::OneToOne {
                    column_reference: "profile_id".to_string(),
                },
            )
            .nav_p(
                "posts",
                "Post",
                NavigationPropertyKind::OneToMany {
                    column_reference: "author_id".to_string(),
                },
            )
            .build();

        let profile_model = ModelBuilder::new("Profile")
            .id_pk()
            .col("bio", CidlType::Text, None)
            .build();

        let post_model = ModelBuilder::new("Post")
            .id_pk()
            .col("title", CidlType::Text, None)
            .col("author_id", CidlType::Integer, Some("User".to_string()))
            .nav_p("tags", "Tag", NavigationPropertyKind::ManyToMany)
            .build();

        let tag_model = ModelBuilder::new("Tag")
            .id_pk()
            .col("name", CidlType::Text, None)
            .nav_p("posts", "Post", NavigationPropertyKind::ManyToMany)
            .build();

        let meta = vec![user_model, profile_model, post_model, tag_model]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect();

        let include_tree = Some(
            json!({
                "profile": {},
                "posts": {
                    "tags": {}
                }
            })
            .as_object()
            .unwrap()
            .clone(),
        );

        let insert_query = r#"
            INSERT INTO Profile (id, bio) VALUES (1, 'Bio of User1'), (2, 'Bio of User2');
            INSERT INTO User (id, username, profile_id) VALUES (1, 'user1', 1), (2, 'user2', 2);
            INSERT INTO Tag (id, name) VALUES (1, 'tag1'), (2, 'tag2'), (3, 'tag3');
            INSERT INTO Post (id, title, author_id) VALUES (1, 'Post 1', 1), (2, 'Post 2', 1), (3, 'Post 3', 2);
            INSERT INTO PostTag ("left", "right") VALUES (1, 1), (1, 2), (2, 2), (3, 3);
        "#;

        // Act
        let expr = select_as_json("User", include_tree, &meta).expect("as_json to work");

        // Assert
        let results = test_sql(
            meta,
            vec![
                (insert_query.to_string(), vec![]),
                (format!("SELECT {} FROM User;", expr), vec![]),
            ],
            db,
        )
        .await
        .expect("Upsert to work");

        let expected = r#"[{"id":1,"username":"user1","profile_id":1,"profile":{"id":1,"bio":"Bio of User1"},"posts":[{"id":1,"title":"Post 1","author_id":1,"tags":[{"id":1,"name":"tag1"},{"id":2,"name":"tag2"}]},{"id":2,"title":"Post 2","author_id":1,"tags":[{"id":2,"name":"tag2"}]}]},{"id":2,"username":"user2","profile_id":2,"profile":{"id":2,"bio":"Bio of User2"},"posts":[{"id":3,"title":"Post 3","author_id":2,"tags":[{"id":3,"name":"tag3"}]}]}]"#;
        let value: String = results[1][0].try_get(0).unwrap();
        expected_str!(value, expected);
    }
}
