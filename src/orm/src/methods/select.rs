use ast::{CloesceAst, Model, NavigationPropertyKind, fail};
use sea_query::{
    Expr, IntoCondition, IntoIden, Query, SelectStatement, SqliteQueryBuilder, TableRef,
};
use serde_json::Value;

use crate::{
    IncludeTreeJson,
    methods::{OrmErrorKind, alias},
};

use super::Result;

pub struct SelectModel<'a> {
    ast: &'a CloesceAst,
    path: Vec<String>,
    gensym_c: usize,
    query: SelectStatement,
}

impl<'a> SelectModel<'a> {
    pub fn query(
        model_name: &str,
        from: Option<String>,
        include_tree: Option<IncludeTreeJson>,
        ast: &'a CloesceAst,
    ) -> Result<String> {
        let model = match ast.models.get(model_name) {
            Some(m) => m,
            None => fail!(OrmErrorKind::UnknownModel, "{}", model_name),
        };
        if model.primary_key_columns.is_empty() {
            fail!(
                OrmErrorKind::ModelMissingD1,
                "Model '{}' is not a D1 model.",
                model_name
            )
        }

        const CUSTOM_FROM: &str = "__custom_from_placeholder__";
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
            ast,
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

        // Primary Key columns
        for pk_col in &model.primary_key_columns {
            let pk_name = &pk_col.value.name;

            let col = if let Some(m2m_alias) = m2m_alias {
                // For M:M tables with composite PKs:
                // - single PK: use "left" or "right" (alphabetically sorted)
                // - composite PK: use "left_<pk_name>" or "right_<pk_name>"
                let base = if model.name.as_str() < m2m_alias.trim_end_matches("_") {
                    "left"
                } else {
                    "right"
                };

                let col_name = if model.primary_key_columns.len() == 1 {
                    base.to_string()
                } else {
                    format!("{}_{}", base, pk_name)
                };

                Expr::col((alias(m2m_alias), alias(&col_name)))
            } else {
                Expr::col((alias(&model_alias), alias(pk_name)))
            };

            self.query.expr_as(col, alias(join_path(pk_name)));
        }

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

            let child = self.ast.models.get(&nav.model_reference).unwrap();
            let child_alias = self.gensym(&child.name);
            let mut child_m2m_alias = None;

            match &nav.kind {
                NavigationPropertyKind::OneToOne { key_columns } => {
                    // Build join condition for all key columns
                    let mut condition = sea_query::Condition::all();

                    for (key_col, pk_col) in
                        key_columns.iter().zip(child.primary_key_columns.iter())
                    {
                        condition = condition.add(
                            Expr::col((alias(&model_alias), alias(key_col)))
                                .equals((alias(&child_alias), alias(&pk_col.value.name))),
                        );
                    }

                    left_join_as(&mut self.query, &child.name, &child_alias, condition);
                }
                NavigationPropertyKind::OneToMany { key_columns } => {
                    // Build join condition for all key columns
                    let mut condition = sea_query::Condition::all();

                    for (pk_col, key_col) in
                        model.primary_key_columns.iter().zip(key_columns.iter())
                    {
                        condition = condition.add(
                            Expr::col((alias(&model_alias), alias(&pk_col.value.name)))
                                .equals((alias(&child_alias), alias(key_col))),
                        );
                    }

                    left_join_as(&mut self.query, &child.name, &child_alias, condition);
                }
                NavigationPropertyKind::ManyToMany => {
                    let m2m_table_name = nav.many_to_many_table_name(&model.name);
                    let m2m_alias = self.gensym(&m2m_table_name);

                    let (side_a, side_b) = if model.name < nav.model_reference {
                        ("left", "right")
                    } else {
                        ("right", "left")
                    };

                    // Join from current model to M:M table
                    // Handle both single and composite primary keys
                    let mut condition_a = sea_query::Condition::all();
                    for pk_col in &model.primary_key_columns {
                        let m2m_col = if model.primary_key_columns.len() == 1 {
                            side_a.to_string()
                        } else {
                            format!("{}_{}", side_a, pk_col.value.name)
                        };

                        condition_a = condition_a.add(
                            Expr::col((alias(&model_alias), alias(&pk_col.value.name)))
                                .equals((alias(&m2m_alias), alias(&m2m_col))),
                        );
                    }

                    left_join_as(&mut self.query, &m2m_table_name, &m2m_alias, condition_a);

                    // Join from M:M table to child model
                    // Handle both single and composite primary keys
                    let mut condition_b = sea_query::Condition::all();
                    for pk_col in &child.primary_key_columns {
                        let m2m_col = if child.primary_key_columns.len() == 1 {
                            side_b.to_string()
                        } else {
                            format!("{}_{}", side_b, pk_col.value.name)
                        };

                        condition_b = condition_b.add(
                            Expr::col((alias(&m2m_alias), alias(&m2m_col)))
                                .equals((alias(&child_alias), alias(&pk_col.value.name))),
                        );
                    }

                    left_join_as(&mut self.query, &child.name, &child_alias, condition_b);

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
    use ast::{CidlType, ForeignKeyReference, NavigationPropertyKind};
    use generator_test::{ModelBuilder, create_ast, expected_str};
    use serde_json::json;
    use sqlx::{Row, SqlitePool};

    use crate::methods::{select::SelectModel, test_sql};

    #[sqlx::test]
    async fn scalar_model(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Person")
            .id_pk()
            .col("name", CidlType::Text, None, None)
            .build();

        let ast = create_ast(vec![ast_model]);

        let insert_query = r#"
            INSERT INTO Person (id, name) VALUES (1, 'Alice'), (2, 'Bob');
        "#
        .to_string();

        // Act
        let select_stmt =
            SelectModel::query("Person", None, None, &ast).expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Person"."id" AS "id", "Person"."name" AS "name" FROM "Person""#
        );

        let results = test_sql(ast, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
            .await
            .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<String, _>("name").unwrap(), "Alice");
    }

    #[sqlx::test]
    async fn one_to_one(db: SqlitePool) {
        // Arrange
        let ast = create_ast(vec![
            ModelBuilder::new("Person")
                .id_pk()
                .col(
                    "dogId",
                    CidlType::Integer,
                    Some(ForeignKeyReference {
                        model_name: "Dog".into(),
                        column_name: "id".into(),
                    }),
                    None,
                )
                .nav_p(
                    "dog",
                    "Dog",
                    NavigationPropertyKind::OneToOne {
                        key_columns: vec!["dogId".into()],
                    },
                )
                .build(),
            ModelBuilder::new("Dog").id_pk().build(),
        ]);

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
            &ast,
        )
        .expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Person"."id" AS "id", "Person"."dogId" AS "dogId", "Dog_1"."id" AS "dog.id" FROM "Person" LEFT JOIN "Dog" AS "Dog_1" ON "Person"."dogId" = "Dog_1"."id""#
        );

        let results = test_sql(ast, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
            .await
            .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("dogId").unwrap(), 1);
    }

    #[sqlx::test]
    fn one_to_many(db: SqlitePool) {
        let ast = create_ast(vec![
            ModelBuilder::new("Dog")
                .id_pk()
                .col(
                    "personId",
                    CidlType::Integer,
                    Some(ForeignKeyReference {
                        model_name: "Person".into(),
                        column_name: "id".into(),
                    }),
                    None,
                )
                .build(),
            ModelBuilder::new("Cat")
                .col(
                    "personId",
                    CidlType::Integer,
                    Some(ForeignKeyReference {
                        model_name: "Person".into(),
                        column_name: "id".into(),
                    }),
                    None,
                )
                .id_pk()
                .build(),
            ModelBuilder::new("Person")
                .id_pk()
                .nav_p(
                    "dogs",
                    "Dog",
                    NavigationPropertyKind::OneToMany {
                        key_columns: vec!["personId".into()],
                    },
                )
                .nav_p(
                    "cats",
                    "Cat",
                    NavigationPropertyKind::OneToMany {
                        key_columns: vec!["personId".into()],
                    },
                )
                .col(
                    "bossId",
                    CidlType::Integer,
                    Some(ForeignKeyReference {
                        model_name: "Boss".into(),
                        column_name: "id".into(),
                    }),
                    None,
                )
                .build(),
            ModelBuilder::new("Boss")
                .id_pk()
                .nav_p(
                    "persons",
                    "Person",
                    NavigationPropertyKind::OneToMany {
                        key_columns: vec!["bossId".into()],
                    },
                )
                .build(),
        ]);

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
            &ast,
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

        let results = test_sql(ast, vec![(insert_query, vec![]), (sql, vec![])], db)
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
        let ast = create_ast(vec![
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
        ]);

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
            &ast,
        )
        .expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Student"."id" AS "id", "CourseStudent_2"."left" AS "courses.id" FROM "Student" LEFT JOIN "CourseStudent" AS "CourseStudent_2" ON "Student"."id" = "CourseStudent_2"."right" LEFT JOIN "Course" AS "Course_1" ON "CourseStudent_2"."left" = "Course_1"."id""#
        );

        let results = test_sql(ast, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
            .await
            .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("courses.id").unwrap(), 1);
    }

    #[sqlx::test]
    async fn composite_one_to_one(db: SqlitePool) {
        // Arrange
        let ast = create_ast(vec![
            ModelBuilder::new("Student")
                .pk("school_id", CidlType::Integer)
                .pk("student_number", CidlType::Integer)
                .col("name", CidlType::Text, None, None)
                .build(),
            ModelBuilder::new("Enrollment")
                .id_pk()
                .col(
                    "school_id",
                    CidlType::Integer,
                    Some(ForeignKeyReference {
                        model_name: "Student".into(),
                        column_name: "school_id".into(),
                    }),
                    Some(0),
                )
                .col(
                    "student_number",
                    CidlType::Integer,
                    Some(ForeignKeyReference {
                        model_name: "Student".into(),
                        column_name: "student_number".into(),
                    }),
                    Some(0),
                )
                .col("course", CidlType::Text, None, None)
                .nav_p(
                    "student",
                    "Student",
                    NavigationPropertyKind::OneToOne {
                        key_columns: vec!["school_id".into(), "student_number".into()],
                    },
                )
                .build(),
        ]);

        let include_tree = json!({
            "student": {}
        });

        let insert_query = r#"
            INSERT INTO Student (school_id, student_number, name) VALUES (10, 5001, 'Alice'), (10, 5002, 'Bob');
            INSERT INTO Enrollment (id, school_id, student_number, course) VALUES (1, 10, 5001, 'Math 101'), (2, 10, 5002, 'Physics 101');
        "#
        .to_string();

        // Act
        let select_stmt = SelectModel::query(
            "Enrollment",
            None,
            Some(include_tree.as_object().unwrap().clone()),
            &ast,
        )
        .expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Enrollment"."id" AS "id", "Enrollment"."school_id" AS "school_id", "Enrollment"."student_number" AS "student_number", "Enrollment"."course" AS "course", "Student_1"."school_id" AS "student.school_id", "Student_1"."student_number" AS "student.student_number", "Student_1"."name" AS "student.name" FROM "Enrollment" LEFT JOIN "Student" AS "Student_1" ON "Enrollment"."school_id" = "Student_1"."school_id" AND "Enrollment"."student_number" = "Student_1"."student_number""#
        );

        let results = test_sql(ast, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
            .await
            .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("school_id").unwrap(), 10);
        assert_eq!(value.try_get::<u32, _>("student_number").unwrap(), 5001);
        assert_eq!(value.try_get::<String, _>("course").unwrap(), "Math 101");
        assert_eq!(value.try_get::<u32, _>("student.school_id").unwrap(), 10);
        assert_eq!(
            value.try_get::<u32, _>("student.student_number").unwrap(),
            5001
        );
        assert_eq!(value.try_get::<String, _>("student.name").unwrap(), "Alice");
    }

    #[sqlx::test]
    async fn composite_one_to_many(db: SqlitePool) {
        // Arrange
        let ast = create_ast(vec![
            ModelBuilder::new("Order")
                .pk("region_id", CidlType::Integer)
                .pk("order_number", CidlType::Integer)
                .col("customer", CidlType::Text, None, None)
                .nav_p(
                    "items",
                    "OrderItem",
                    NavigationPropertyKind::OneToMany {
                        key_columns: vec!["region_id".into(), "order_number".into()],
                    },
                )
                .build(),
            ModelBuilder::new("OrderItem")
                .id_pk()
                .col(
                    "region_id",
                    CidlType::Integer,
                    Some(ForeignKeyReference {
                        model_name: "Order".into(),
                        column_name: "region_id".into(),
                    }),
                    Some(0),
                )
                .col(
                    "order_number",
                    CidlType::Integer,
                    Some(ForeignKeyReference {
                        model_name: "Order".into(),
                        column_name: "order_number".into(),
                    }),
                    Some(0),
                )
                .col("product", CidlType::Text, None, None)
                .build(),
        ]);

        let include_tree = json!({
            "items": {}
        });

        let insert_query = r#"
            INSERT INTO "Order" (region_id, order_number, customer) VALUES (1, 100, 'Bob');
            INSERT INTO OrderItem (id, region_id, order_number, product) VALUES (1, 1, 100, 'Widget'), (2, 1, 100, 'Gadget');
        "#
        .to_string();

        // Act
        let select_stmt = SelectModel::query(
            "Order",
            None,
            Some(include_tree.as_object().unwrap().clone()),
            &ast,
        )
        .expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Order"."region_id" AS "region_id", "Order"."order_number" AS "order_number", "Order"."customer" AS "customer", "OrderItem_1"."id" AS "items.id", "OrderItem_1"."region_id" AS "items.region_id", "OrderItem_1"."order_number" AS "items.order_number", "OrderItem_1"."product" AS "items.product" FROM "Order" LEFT JOIN "OrderItem" AS "OrderItem_1" ON "Order"."region_id" = "OrderItem_1"."region_id" AND "Order"."order_number" = "OrderItem_1"."order_number""#
        );

        let results = test_sql(ast, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
            .await
            .expect("SQL to execute");

        let value1 = &results[1][0];
        assert_eq!(value1.try_get::<u32, _>("region_id").unwrap(), 1);
        assert_eq!(value1.try_get::<u32, _>("order_number").unwrap(), 100);
        assert_eq!(value1.try_get::<String, _>("customer").unwrap(), "Bob");
        assert_eq!(value1.try_get::<u32, _>("items.id").unwrap(), 2);
        assert_eq!(value1.try_get::<u32, _>("items.region_id").unwrap(), 1);
        assert_eq!(value1.try_get::<u32, _>("items.order_number").unwrap(), 100);
        assert_eq!(
            value1.try_get::<String, _>("items.product").unwrap(),
            "Gadget"
        );

        let value2 = &results[1][1];
        assert_eq!(value2.try_get::<u32, _>("region_id").unwrap(), 1);
        assert_eq!(value2.try_get::<u32, _>("order_number").unwrap(), 100);
        assert_eq!(value2.try_get::<String, _>("customer").unwrap(), "Bob");
        assert_eq!(value2.try_get::<u32, _>("items.id").unwrap(), 1);
        assert_eq!(value2.try_get::<u32, _>("items.region_id").unwrap(), 1);
        assert_eq!(value2.try_get::<u32, _>("items.order_number").unwrap(), 100);
        assert_eq!(
            value2.try_get::<String, _>("items.product").unwrap(),
            "Widget"
        );
    }

    #[sqlx::test]
    async fn composite_many_to_many(db: SqlitePool) {
        // Arrange
        let ast = create_ast(vec![
            ModelBuilder::new("Teacher")
                .pk("school_id", CidlType::Integer)
                .pk("employee_id", CidlType::Integer)
                .col("name", CidlType::Text, None, None)
                .nav_p("courses", "Course", NavigationPropertyKind::ManyToMany)
                .build(),
            ModelBuilder::new("Course")
                .pk("department_id", CidlType::Integer)
                .pk("course_code", CidlType::Integer)
                .col("title", CidlType::Text, None, None)
                .nav_p("teachers", "Teacher", NavigationPropertyKind::ManyToMany)
                .build(),
        ]);

        let include_tree = json!({
            "courses": {}
        });

        let insert_query = r#"
            INSERT INTO Teacher (school_id, employee_id, name) VALUES (1, 123, 'Dr. Smith');
            INSERT INTO Course (department_id, course_code, title) VALUES (10, 101, 'Intro to CS');
            INSERT INTO CourseTeacher (left_department_id, left_course_code, right_school_id, right_employee_id)
            VALUES (10, 101, 1, 123);
        "#
        .to_string();

        // Act
        let select_stmt = SelectModel::query(
            "Teacher",
            None,
            Some(include_tree.as_object().unwrap().clone()),
            &ast,
        )
        .expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Teacher"."school_id" AS "school_id", "Teacher"."employee_id" AS "employee_id", "Teacher"."name" AS "name", "CourseTeacher_2"."left_department_id" AS "courses.department_id", "CourseTeacher_2"."left_course_code" AS "courses.course_code", "Course_1"."title" AS "courses.title" FROM "Teacher" LEFT JOIN "CourseTeacher" AS "CourseTeacher_2" ON "Teacher"."school_id" = "CourseTeacher_2"."right_school_id" AND "Teacher"."employee_id" = "CourseTeacher_2"."right_employee_id" LEFT JOIN "Course" AS "Course_1" ON "CourseTeacher_2"."left_department_id" = "Course_1"."department_id" AND "CourseTeacher_2"."left_course_code" = "Course_1"."course_code""#
        );

        let results = test_sql(ast, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
            .await
            .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("school_id").unwrap(), 1);
        assert_eq!(value.try_get::<u32, _>("employee_id").unwrap(), 123);
        assert_eq!(value.try_get::<String, _>("name").unwrap(), "Dr. Smith");
        assert_eq!(
            value.try_get::<u32, _>("courses.department_id").unwrap(),
            10
        );
        assert_eq!(value.try_get::<u32, _>("courses.course_code").unwrap(), 101);
        assert_eq!(
            value.try_get::<String, _>("courses.title").unwrap(),
            "Intro to CS"
        );
    }

    #[sqlx::test]
    async fn gensym_stops_ambigious_table(db: SqlitePool) {
        // Arrange
        let horse_model = ModelBuilder::new("Horse")
            .id_pk()
            .col("name", CidlType::Text, None, None)
            .col("bio", CidlType::nullable(CidlType::Text), None, None)
            .nav_p(
                "matches",
                "Match",
                NavigationPropertyKind::OneToMany {
                    key_columns: vec!["horseId1".into()],
                },
            )
            .build();

        let match_model = ModelBuilder::new("Match")
            .id_pk()
            .col(
                "horseId1",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Horse".into(),
                    column_name: "id".into(),
                }),
                None,
            )
            .col(
                "horseId2",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Horse".into(),
                    column_name: "id".into(),
                }),
                None,
            )
            .nav_p(
                "horse2",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    key_columns: vec!["horseId2".into()],
                },
            )
            .build();

        let ast = create_ast(vec![horse_model, match_model]);
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
            &ast,
        )
        .expect("list models to work");

        // Assert
        expected_str!(
            sql,
            r#"SELECT "Horse"."id" AS "id", "Horse"."name" AS "name", "Horse"."bio" AS "bio", "Match_1"."id" AS "matches.id", "Match_1"."horseId1" AS "matches.horseId1", "Match_1"."horseId2" AS "matches.horseId2", "Horse_2"."id" AS "matches.horse2.id", "Horse_2"."name" AS "matches.horse2.name", "Horse_2"."bio" AS "matches.horse2.bio" FROM "Horse" LEFT JOIN "Match" AS "Match_1" ON "Horse"."id" = "Match_1"."horseId1" LEFT JOIN "Horse" AS "Horse_2" ON "Match_1"."horseId2" = "Horse_2"."id""#
        );

        let results = test_sql(ast, vec![(insert_query, vec![]), (sql, vec![])], db)
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
            .col("name", CidlType::Text, None, None)
            .build();

        let ast = create_ast(vec![ast_model]);

        let insert_query = r#"
            INSERT INTO Person (id, name) VALUES (1, 'Alice'), (2, 'Bob');
        "#
        .to_string();

        // Act
        let custom_from = "SELECT * FROM Person WHERE name = 'Alice'".to_string();
        let select_stmt = SelectModel::query("Person", Some(custom_from), None, &ast)
            .expect("SelectModel::query to work");

        // Assert
        expected_str!(
            select_stmt,
            r#"SELECT "Person"."id" AS "id", "Person"."name" AS "name" FROM (SELECT * FROM Person WHERE name = 'Alice') AS "Person""#
        );

        let results = test_sql(ast, vec![(insert_query, vec![]), (select_stmt, vec![])], db)
            .await
            .expect("SQL to execute");

        let value = &results[1][0];
        assert_eq!(value.try_get::<u32, _>("id").unwrap(), 1);
        assert_eq!(value.try_get::<String, _>("name").unwrap(), "Alice");
    }
}
