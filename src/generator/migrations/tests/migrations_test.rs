use std::collections::HashMap;

use ast::{CidlType, CloesceAst, MigrationsAst, MigrationsModel, NavigationPropertyKind};
use generator_test::{IncludeTreeBuilder, ModelBuilder, create_ast, expected_str};
use migrations::{MigrationsDilemma, MigrationsGenerator, MigrationsIntent};

use indexmap::IndexMap;
use sqlx::SqlitePool;

async fn exists_in_db(db: &SqlitePool, name: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) 
         FROM sqlite_master 
         WHERE type='table' AND name=?1",
    )
    .bind(name)
    .fetch_one(db)
    .await
    .expect("Failed to check object existence")
        > 0
}

async fn query(db: &SqlitePool, sql: &str) -> Result<(), sqlx::Error> {
    let tx = db.begin().await?;
    sqlx::query(sql).execute(db).await?;
    tx.commit().await?;
    Ok(())
}

fn as_migration(ast: CloesceAst) -> MigrationsAst {
    let CloesceAst { hash, models, .. } = ast;

    // Convert each full Model → MigrationsModel
    let migrations_models: IndexMap<String, MigrationsModel> = models
        .into_iter()
        .map(|(name, model)| {
            let m = MigrationsModel {
                hash: model.hash,
                name: model.name,
                primary_key: model.primary_key.unwrap(),
                columns: model.columns,
                navigation_properties: model.navigation_properties,
            };
            (name, m)
        })
        .collect();

    MigrationsAst {
        hash,
        models: migrations_models,
    }
}

fn empty_migration() -> MigrationsAst {
    let mut empty_ast = create_ast(vec![]);
    empty_ast.set_merkle_hash();
    as_migration(empty_ast)
}

#[derive(Default)]
struct MockMigrationsIntent {
    answers: HashMap<(String, Option<String>), Option<String>>,
}

impl MigrationsIntent for MockMigrationsIntent {
    fn ask(&self, dilemma: MigrationsDilemma) -> Option<usize> {
        let (key, opts) = match &dilemma {
            MigrationsDilemma::RenameOrDropModel {
                model_name,
                options,
            } => ((model_name.clone(), None), options),
            MigrationsDilemma::RenameOrDropColumn {
                model_name,
                options,
                column_name: attribute_name,
            } => ((model_name.clone(), Some(attribute_name.clone())), options),
        };

        let ans = self.answers.get(&key).unwrap().clone();
        ans.map(|a| opts.iter().enumerate().find(|(_, o)| ***o == a).unwrap().0)
    }
}

#[sqlx::test]
async fn migrate_models_scalars(db: SqlitePool) {
    let empty_ast = empty_migration();

    // Create
    let ast = {
        // Arrange
        let ast = {
            let mut ast = create_ast(vec![
                ModelBuilder::new("User")
                    .id_pk()
                    .col("name", CidlType::nullable(CidlType::Text), None)
                    .col("age", CidlType::Integer, None)
                    .col("address", CidlType::Text, None)
                    .build(),
            ]);
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql =
            MigrationsGenerator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, "CREATE TABLE IF NOT EXISTS");
        expected_str!(sql, "\"id\" integer PRIMARY KEY");
        expected_str!(sql, "\"name\" text");
        expected_str!(sql, "\"age\" integer NOT NULL");
        expected_str!(sql, "\"address\" text NOT NULL");

        query(&db, &sql).await.expect("Insert table query to work");
        assert!(exists_in_db(&db, "User").await);

        ast
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, "DROP TABLE IF EXISTS \"User\"");

        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "User").await);
    }
}

#[sqlx::test]
async fn migrate_models_one_to_one(db: SqlitePool) {
    let empty_ast = empty_migration();

    // Create
    let ast = {
        // Arrange
        let ast = {
            let mut ast = create_ast(vec![
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
                    .data_source(
                        "default",
                        IncludeTreeBuilder::default().add_node("dog").build(),
                        false,
                    )
                    .build(),
                ModelBuilder::new("Dog").id_pk().build(),
            ]);
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql =
            MigrationsGenerator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );

        query(&db, &sql).await.expect("Insert query to work");
        assert!(exists_in_db(&db, "Person").await);
        assert!(exists_in_db(&db, "Dog").await);

        ast
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        // Assert
        query(&db, &sql).await.expect("Drop query to work");

        assert!(!exists_in_db(&db, "Person").await);
        assert!(!exists_in_db(&db, "Dog").await);
    }
}

#[sqlx::test]
async fn migrate_models_one_to_many(db: SqlitePool) {
    let empty_ast = empty_migration();

    // Create
    let ast = {
        // Arrange
        let ast = {
            let mut ast = create_ast(vec![
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
                    .data_source(
                        "default",
                        IncludeTreeBuilder::default()
                            .add_node("dogs")
                            .add_node("cats")
                            .build(),
                        false,
                    )
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
                    .data_source(
                        "default",
                        IncludeTreeBuilder::default()
                            .add_with_children("persons", |b| b.add_node("dogs").add_node("cats"))
                            .build(),
                        false,
                    )
                    .build(),
            ]);
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql =
            MigrationsGenerator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(
            sql,
            r#"CREATE TABLE IF NOT EXISTS "Boss" ( "id" integer PRIMARY KEY );"#
        );
        expected_str!(
            sql,
            r#"CREATE TABLE IF NOT EXISTS "Person" ( "id" integer PRIMARY KEY, "bossId" integer NOT NULL, FOREIGN KEY ("bossId") REFERENCES "Boss" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
        );
        expected_str!(
            sql,
            r#"CREATE TABLE IF NOT EXISTS "Dog" ( "id" integer PRIMARY KEY, "personId" integer NOT NULL, FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
        );
        expected_str!(
            sql,
            r#"CREATE TABLE IF NOT EXISTS "Cat" ( "id" integer PRIMARY KEY, "personId" integer NOT NULL, FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
        );

        query(&db, &sql).await.expect("Insert query to work");
        assert!(exists_in_db(&db, "Boss").await);
        assert!(exists_in_db(&db, "Person").await);
        assert!(exists_in_db(&db, "Dog").await);
        assert!(exists_in_db(&db, "Cat").await);

        ast
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "Boss").await);
        assert!(!exists_in_db(&db, "Person").await);
        assert!(!exists_in_db(&db, "Dog").await);
        assert!(!exists_in_db(&db, "Cat").await);
    }
}

#[sqlx::test]
async fn migrate_models_many_to_many(db: SqlitePool) {
    let empty_ast = empty_migration();

    // Create
    let ast = {
        // Arrange
        let ast = {
            let mut ast = create_ast(vec![
                ModelBuilder::new("Student")
                    .id_pk()
                    .nav_p(
                        "courses",
                        "Course".to_string(),
                        NavigationPropertyKind::ManyToMany,
                    )
                    .data_source(
                        "withCourses",
                        IncludeTreeBuilder::default().add_node("courses").build(),
                        false,
                    )
                    .build(),
                ModelBuilder::new("Course")
                    .id_pk()
                    .nav_p(
                        "students",
                        "Student".to_string(),
                        NavigationPropertyKind::ManyToMany,
                    )
                    .data_source(
                        "withStudents",
                        IncludeTreeBuilder::default().add_node("students").build(),
                        false,
                    )
                    .build(),
            ]);
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql =
            MigrationsGenerator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, r#"CREATE TABLE IF NOT EXISTS "CourseStudent""#);
        expected_str!(sql, r#""left" integer NOT NULL"#);
        expected_str!(sql, r#""right" integer NOT NULL"#);
        expected_str!(sql, r#"PRIMARY KEY ("left", "right")"#);
        expected_str!(
            sql,
            r#"FOREIGN KEY ("left") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
        );
        expected_str!(
            sql,
            r#"FOREIGN KEY ("right") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
        );

        query(&db, &sql).await.expect("Insert tables query to work");
        assert!(exists_in_db(&db, "CourseStudent").await);

        ast
    };

    // Drop
    {
        // Act
        let sql =
            MigrationsGenerator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        // Assert
        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "StudentsCourses").await);
    }
}

#[sqlx::test]
async fn migrate_with_rebuild(db: SqlitePool) {
    let mut base_ast = {
        let ast = as_migration(create_ast(vec![
            ModelBuilder::new("User")
                .id_pk()
                .col("name", CidlType::nullable(CidlType::Text), None)
                .col("age", CidlType::Integer, None)
                .col("address", CidlType::Text, None)
                .build(),
        ]));

        let sql = MigrationsGenerator::migrate(&ast, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");

        ast
    };

    // Changes without Rebuild
    base_ast = {
        // Arrange
        let new = {
            let mut ast = create_ast(vec![
                ModelBuilder::new("User")
                    .id_pk()
                    .col("first_name", CidlType::nullable(CidlType::Text), None) // changed name
                    .col("age", CidlType::Text, None) // changed type
                    .col("favorite_color", CidlType::Text, None) // added column
                    // dropped column "address"
                    .build(),
            ]);
            ast.set_merkle_hash();
            as_migration(ast)
        };

        let mut intent = MockMigrationsIntent::default();
        intent.answers.insert(
            ("User".into(), Some("name".into())),
            Some("first_name".into()),
        );
        intent
            .answers
            .insert(("User".into(), Some("address".into())), None);

        // Act
        let sql = MigrationsGenerator::migrate(&new, Some(&base_ast), &intent);

        // Assert
        expected_str!(
            sql,
            "ALTER TABLE \"User\" RENAME COLUMN \"name\" TO \"first_name\""
        );
        expected_str!(
            sql,
            r#"
ALTER TABLE "User" DROP COLUMN "age";
ALTER TABLE "User" ADD COLUMN "age" text"#
        );
        expected_str!(sql, "ALTER TABLE \"User\" DROP COLUMN \"address\"");

        query(&db, &sql).await.expect("Alter table queries to work");
        assert!(exists_in_db(&db, "User").await);

        new
    };

    // Rebuild: Primary Key
    base_ast = {
        // Arrange
        let new = {
            let mut ast = create_ast(vec![
                ModelBuilder::new("User")
                    .id_pk()
                    .col("first_name", CidlType::nullable(CidlType::Text), None)
                    .col("age", CidlType::Text, None)
                    .col("favorite_color", CidlType::Text, None)
                    .build(),
            ]);
            ast.models[0].primary_key.as_mut().unwrap().cidl_type = CidlType::Text; // new PK type
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql =
            MigrationsGenerator::migrate(&new, Some(&base_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, r#"ALTER TABLE "User" RENAME TO "User_"#);
        expected_str!(
            sql,
            r#"CREATE TABLE IF NOT EXISTS "User" ( "id" text PRIMARY KEY, "first_name" text, "age" text NOT NULL, "favorite_color" text NOT NULL );"#
        );
        expected_str!(
            sql,
            r#"INSERT INTO "User" ("first_name", "age", "favorite_color", "id") SELECT "first_name", "age", "favorite_color", CAST("id" AS text) FROM "User"#
        );
        expected_str!(sql, r#"DROP TABLE "User_"#);

        query(&db, &sql).await.expect("Alter table queries to work");
        assert!(exists_in_db(&db, "User").await);

        new
    };

    // Rebuild: Foreign Key
    {
        // Arrange
        let new = {
            let mut ast = create_ast(vec![
                ModelBuilder::new("Dog").id_pk().build(), // added Dog
                ModelBuilder::new("User")
                    .id_pk()
                    .col("first_name", CidlType::nullable(CidlType::Text), None)
                    .col("age", CidlType::Text, None)
                    .col("favorite_color", CidlType::Text, None)
                    .col("dog_id", CidlType::Integer, Some("Dog".into())) // added Dog FK
                    .build(),
            ]);
            ast.models[1].primary_key.as_mut().unwrap().cidl_type = CidlType::Text;
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql =
            MigrationsGenerator::migrate(&new, Some(&base_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, r#"ALTER TABLE "User" RENAME TO "User_"#);
        expected_str!(
            sql,
            r#"INSERT INTO "User" ("first_name", "age", "favorite_color", "dog_id", "id") SELECT "first_name", "age", "favorite_color", 0, "id" FROM "User_"#
        );
        expected_str!(sql, r#"DROP TABLE "User_"#);

        query(&db, &sql).await.expect("Alter table queries to work");
        assert!(exists_in_db(&db, "User").await);
        assert!(exists_in_db(&db, "Dog").await);
    }
}

#[sqlx::test]
async fn migrate_with_rename(db: SqlitePool) {
    // Arrange
    let base_ast = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("User")
                .id_pk()
                .col("name", CidlType::nullable(CidlType::Text), None)
                .col("age", CidlType::Integer, None)
                .col("address", CidlType::Text, None)
                .build(),
            ModelBuilder::new("UserSettings")
                .id_pk()
                .col("userId", CidlType::Integer, Some("User".into()))
                .build(),
        ]);
        ast.set_merkle_hash();
        let migration = as_migration(ast);

        let sql = MigrationsGenerator::migrate(&migration, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");

        migration
    };

    let new = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("AppUser")
                .id_pk()
                .col("name", CidlType::nullable(CidlType::Text), None)
                .col("age", CidlType::Integer, None)
                .col("address", CidlType::Text, None)
                .build(),
            ModelBuilder::new("UserSettings")
                .id_pk()
                .col("userId", CidlType::Integer, Some("AppUser".into()))
                .build(),
        ]);
        ast.set_merkle_hash();
        as_migration(ast)
    };

    let mut intent = MockMigrationsIntent::default();
    intent
        .answers
        .insert(("User".into(), None), Some("AppUser".into()));

    // Act
    let sql = MigrationsGenerator::migrate(&new, Some(&base_ast), &intent);

    // Assert
    expected_str!(sql, r#"ALTER TABLE "User" RENAME TO "AppUser""#);
    assert!(sql.matches('\n').count() < 5); // should have less than 5 newlines (i.e., should be a single ALTER TABLE statement)

    query(&db, &sql).await.expect("Alter table queries to work");
    assert!(exists_in_db(&db, "AppUser").await);
    assert!(!exists_in_db(&db, "User").await);
}

#[sqlx::test]
async fn migrate_alter_drop_m2m(db: SqlitePool) {
    // Arrange
    let m2m_ast = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("Student")
                .id_pk()
                .nav_p("courses", "Course", NavigationPropertyKind::ManyToMany)
                .build(),
            ModelBuilder::new("Course")
                .id_pk()
                .nav_p("students", "Student", NavigationPropertyKind::ManyToMany)
                .build(),
        ]);
        ast.set_merkle_hash();
        let migration = as_migration(ast);

        let sql = MigrationsGenerator::migrate(&migration, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");
        assert!(exists_in_db(&db, "CourseStudent").await);

        migration
    };

    let no_m2m_ast = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("Student").id_pk().build(),
            ModelBuilder::new("Course").id_pk().build(),
        ]);
        ast.set_merkle_hash();
        as_migration(ast)
    };

    // Act
    let sql = MigrationsGenerator::migrate(
        &no_m2m_ast,
        Some(&m2m_ast),
        &MockMigrationsIntent::default(),
    );

    // Assert
    query(&db, &sql)
        .await
        .expect("Create table queries to work");

    assert!(!exists_in_db(&db, "CourseStudent").await)
}

#[sqlx::test]
async fn migrate_alter_add_m2m(db: SqlitePool) {
    // Arrange
    let no_m2m_ast = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("Student").id_pk().build(),
            ModelBuilder::new("Course").id_pk().build(),
        ]);
        ast.set_merkle_hash();
        let migration = as_migration(ast);

        let sql = MigrationsGenerator::migrate(&migration, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");

        migration
    };

    let m2m_ast = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("Student")
                .id_pk()
                .nav_p("courses", "Course", NavigationPropertyKind::ManyToMany)
                .build(),
            ModelBuilder::new("Course")
                .id_pk()
                .nav_p("students", "Student", NavigationPropertyKind::ManyToMany)
                .build(),
        ]);
        ast.set_merkle_hash();
        as_migration(ast)
    };

    // Act
    let sql = MigrationsGenerator::migrate(
        &m2m_ast,
        Some(&no_m2m_ast),
        &MockMigrationsIntent::default(),
    );

    // Assert
    query(&db, &sql)
        .await
        .expect("Create table queries to work");

    assert!(exists_in_db(&db, "CourseStudent").await)
}
