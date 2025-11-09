use std::collections::HashMap;

use ast::{
    CidlType, CloesceAst, MigrationsAst, MigrationsModel, NavigationPropertyKind,
    builder::{IncludeTreeBuilder, ModelBuilder, create_ast},
};

use d1::{D1Generator, MigrationsIntent};
use indexmap::IndexMap;
use sqlx::SqlitePool;

/// Compares two strings disregarding tabs, amount of spaces, and amount of newlines.
/// Ensures that some expr is present in another expr.
macro_rules! expected_str {
    ($got:expr, $expected:expr) => {{
        let clean = |s: &str| s.chars().filter(|c| !c.is_whitespace()).collect::<String>();
        assert!(
            clean(&$got.to_string()).contains(&clean(&$expected.to_string())),
            "Expected:\n`{}`\n\ngot:\n`{}`",
            $expected,
            $got
        );
    }};
}

async fn exists_in_db(db: &SqlitePool, name: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) 
         FROM sqlite_master 
         WHERE (type='table' OR type='view') AND name=?1",
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
                primary_key: model.primary_key,
                attributes: model.attributes,
                navigation_properties: model.navigation_properties,
                data_sources: model.data_sources,
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
    fn ask(&self, dilemma: d1::MigrationsDilemma) -> Option<usize> {
        let (key, opts) = match &dilemma {
            d1::MigrationsDilemma::RenameOrDropModel {
                model_name,
                options,
            } => ((model_name.clone(), None), options),
            d1::MigrationsDilemma::RenameOrDropAttribute {
                model_name,
                options,
                attribute_name,
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
                    .id()
                    .attribute("name", CidlType::nullable(CidlType::Text), None)
                    .attribute("age", CidlType::Integer, None)
                    .attribute("address", CidlType::Text, None)
                    .build(),
            ]);
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql = D1Generator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

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
        let sql = D1Generator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

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
                    .id()
                    .attribute("dogId", CidlType::Integer, Some("Dog".into()))
                    .nav_p(
                        "dog",
                        "Dog",
                        NavigationPropertyKind::OneToOne {
                            reference: "dogId".into(),
                        },
                    )
                    .data_source(
                        "default",
                        IncludeTreeBuilder::default().add_node("dog").build(),
                    )
                    .build(),
                ModelBuilder::new("Dog").id().build(),
            ]);
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql = D1Generator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );
        expected_str!(
            sql,
            r#"
CREATE VIEW IF NOT EXISTS "Person.default" AS
SELECT
  "Person"."id" AS "id",
  "Person"."dogId" AS "dogId",
  "Dog"."id" AS "dog.id"
FROM
  "Person"
  LEFT JOIN "Dog" ON "Person"."dogId" = "Dog"."id";
"#
        );

        query(&db, &sql).await.expect("Insert query to work");
        assert!(exists_in_db(&db, "Person").await);
        assert!(exists_in_db(&db, "Dog").await);
        assert!(exists_in_db(&db, "Person.default").await);

        sqlx::query("SELECT * FROM [Person.default]")
            .execute(&db)
            .await
            .expect("Select query to work");

        ast
    };

    // Drop
    {
        // Act
        let sql = D1Generator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        // Assert
        query(&db, &sql).await.expect("Drop query to work");

        assert!(!exists_in_db(&db, "Person").await);
        assert!(!exists_in_db(&db, "Dog").await);
        assert!(!exists_in_db(&db, "Person.default").await);
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
                    .data_source(
                        "default",
                        IncludeTreeBuilder::default()
                            .add_node("dogs")
                            .add_node("cats")
                            .build(),
                    )
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
                    .data_source(
                        "default",
                        IncludeTreeBuilder::default()
                            .add_with_children("persons", |b| b.add_node("dogs").add_node("cats"))
                            .build(),
                    )
                    .build(),
            ]);
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql = D1Generator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

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

        expected_str!(
            sql,
            r#"
CREATE VIEW IF NOT EXISTS "Person.default" AS
SELECT
  "Person"."id" AS "id",
  "Person"."bossId" AS "bossId",
  "Dog"."id" AS "dogs.id",
  "Dog"."personId" AS "dogs.personId",
  "Cat"."id" AS "cats.id",
  "Cat"."personId" AS "cats.personId"
FROM
  "Person"
  LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId"
  LEFT JOIN "Cat" ON "Person"."id" = "Cat"."personId";
"#
        );
        expected_str!(
            sql,
            r#"
CREATE VIEW IF NOT EXISTS "Boss.default" AS
SELECT
    "Boss"."id" AS "id",
    "Person"."id" AS "persons.id",
    "Person"."bossId" AS "persons.bossId",
    "Dog"."id" AS "persons.dogs.id",
    "Dog"."personId" AS "persons.dogs.personId",
    "Cat"."id" AS "persons.cats.id",
    "Cat"."personId" AS "persons.cats.personId"
FROM
    "Boss"
    LEFT JOIN "Person" ON "Boss"."id" = "Person"."bossId"
    LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId"
    LEFT JOIN "Cat" ON "Person"."id" = "Cat"."personId";
"#
        );

        query(&db, &sql).await.expect("Insert query to work");
        assert!(exists_in_db(&db, "Boss").await);
        assert!(exists_in_db(&db, "Person").await);
        assert!(exists_in_db(&db, "Dog").await);
        assert!(exists_in_db(&db, "Cat").await);
        assert!(exists_in_db(&db, "Person.default").await);
        assert!(exists_in_db(&db, "Boss.default").await);

        sqlx::query("SELECT * FROM [Person.default]")
            .execute(&db)
            .await
            .expect("Select query to work");

        sqlx::query("SELECT * FROM [Boss.default]")
            .execute(&db)
            .await
            .expect("Select query to work");

        ast
    };

    // Drop
    {
        // Act
        let sql = D1Generator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "Boss").await);
        assert!(!exists_in_db(&db, "Person").await);
        assert!(!exists_in_db(&db, "Dog").await);
        assert!(!exists_in_db(&db, "Cat").await);
        assert!(!exists_in_db(&db, "Person.default").await);
        assert!(!exists_in_db(&db, "Boss.default").await);
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
                    .data_source(
                        "withStudents",
                        IncludeTreeBuilder::default().add_node("students").build(),
                    )
                    .build(),
            ]);
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql = D1Generator::migrate(&ast, Some(&empty_ast), &MockMigrationsIntent::default());

        // Assert
        expected_str!(sql, r#"CREATE TABLE IF NOT EXISTS "StudentsCourses""#);
        expected_str!(sql, r#""Student.id" integer NOT NULL"#);
        expected_str!(sql, r#""Course.id" integer NOT NULL"#);
        expected_str!(sql, r#"PRIMARY KEY ("Student.id", "Course.id")"#);
        expected_str!(
            sql,
            r#"FOREIGN KEY ("Student.id") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
        );
        expected_str!(
            sql,
            r#"FOREIGN KEY ("Course.id") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
        );

        expected_str!(
            sql,
            r#"
CREATE VIEW IF NOT EXISTS "Student.withCourses" AS
SELECT
  "Student"."id" AS "id",
  "StudentsCourses"."Course.id" AS "courses.id"
FROM
  "Student"
  LEFT JOIN "StudentsCourses" ON "Student"."id" = "StudentsCourses"."Student.id"
  LEFT JOIN "Course" ON "StudentsCourses"."Course.id" = "Course"."id";
"#
        );

        expected_str!(
            sql,
            r#"
CREATE VIEW IF NOT EXISTS "Course.withStudents" AS
SELECT
    "Course"."id" AS "id",
    "StudentsCourses"."Student.id" AS "students.id"
FROM
    "Course"
    LEFT JOIN "StudentsCourses" ON "Course"."id" = "StudentsCourses"."Course.id"
    LEFT JOIN "Student" ON "StudentsCourses"."Student.id" = "Student"."id";
"#
        );

        query(&db, &sql).await.expect("Insert tables query to work");
        assert!(exists_in_db(&db, "StudentsCourses").await);
        assert!(exists_in_db(&db, "Course.withStudents").await);
        assert!(exists_in_db(&db, "Student.withCourses").await);

        sqlx::query("SELECT * FROM [Course.withStudents]")
            .execute(&db)
            .await
            .expect("Select query to work");
        sqlx::query("SELECT * FROM [Student.withCourses]")
            .execute(&db)
            .await
            .expect("Select query to work");

        ast
    };

    // Drop
    {
        // Act
        let sql = D1Generator::migrate(&empty_ast, Some(&ast), &MockMigrationsIntent::default());

        // Assert
        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "StudentsCourses").await);
    }
}

#[sqlx::test]
async fn migrate_with_alterations(db: SqlitePool) {
    let mut base_ast = {
        let ast = as_migration(create_ast(vec![
            ModelBuilder::new("User")
                .id()
                .attribute("name", CidlType::nullable(CidlType::Text), None)
                .attribute("age", CidlType::Integer, None)
                .attribute("address", CidlType::Text, None)
                .build(),
        ]));

        let sql = D1Generator::migrate(&ast, None, &MockMigrationsIntent::default());
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
                    .id()
                    .attribute("first_name", CidlType::nullable(CidlType::Text), None) // changed name
                    .attribute("age", CidlType::Text, None) // changed type
                    .attribute("favorite_color", CidlType::Text, None) // added column
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
        let sql = D1Generator::migrate(&new, Some(&base_ast), &intent);

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
                    .id()
                    .attribute("first_name", CidlType::nullable(CidlType::Text), None)
                    .attribute("age", CidlType::Text, None)
                    .attribute("favorite_color", CidlType::Text, None)
                    .build(),
            ]);
            ast.models[0].primary_key.cidl_type = CidlType::Text; // new PK type
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql = D1Generator::migrate(&new, Some(&base_ast), &MockMigrationsIntent::default());

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
                ModelBuilder::new("Dog").id().build(), // added Dog
                ModelBuilder::new("User")
                    .id()
                    .attribute("first_name", CidlType::nullable(CidlType::Text), None)
                    .attribute("age", CidlType::Text, None)
                    .attribute("favorite_color", CidlType::Text, None)
                    .attribute("dog_id", CidlType::Integer, Some("Dog".into())) // added Dog FK
                    .build(),
            ]);
            ast.models[1].primary_key.cidl_type = CidlType::Text;
            ast.set_merkle_hash();
            as_migration(ast)
        };

        // Act
        let sql = D1Generator::migrate(&new, Some(&base_ast), &MockMigrationsIntent::default());

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
async fn migrate_alter_drop_m2m(db: SqlitePool) {
    // Arrange
    let m2m_ast = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("Student")
                .id()
                .nav_p(
                    "courses",
                    "Course",
                    NavigationPropertyKind::ManyToMany {
                        unique_id: "StudentsCourses".into(),
                    },
                )
                .build(),
            ModelBuilder::new("Course")
                .id()
                .nav_p(
                    "students",
                    "Student",
                    NavigationPropertyKind::ManyToMany {
                        unique_id: "StudentsCourses".into(),
                    },
                )
                .build(),
        ]);
        ast.set_merkle_hash();
        let migration = as_migration(ast);

        let sql = D1Generator::migrate(&migration, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");
        assert!(exists_in_db(&db, "StudentsCourses").await);

        migration
    };

    let no_m2m_ast = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("Student").id().build(),
            ModelBuilder::new("Course").id().build(),
        ]);
        ast.set_merkle_hash();
        as_migration(ast)
    };

    // Act
    let sql = D1Generator::migrate(
        &no_m2m_ast,
        Some(&m2m_ast),
        &MockMigrationsIntent::default(),
    );

    // Assert
    query(&db, &sql)
        .await
        .expect("Create table queries to work");

    assert!(!exists_in_db(&db, "StudentsCourses").await)
}

#[sqlx::test]
async fn migrate_alter_add_m2m(db: SqlitePool) {
    // Arrange
    let no_m2m_ast = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("Student").id().build(),
            ModelBuilder::new("Course").id().build(),
        ]);
        ast.set_merkle_hash();
        let migration = as_migration(ast);

        let sql = D1Generator::migrate(&migration, None, &MockMigrationsIntent::default());
        query(&db, &sql)
            .await
            .expect("Create table queries to work");

        migration
    };

    let m2m_ast = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("Student")
                .id()
                .nav_p(
                    "courses",
                    "Course",
                    NavigationPropertyKind::ManyToMany {
                        unique_id: "StudentsCourses".into(),
                    },
                )
                .build(),
            ModelBuilder::new("Course")
                .id()
                .nav_p(
                    "students",
                    "Student",
                    NavigationPropertyKind::ManyToMany {
                        unique_id: "StudentsCourses".into(),
                    },
                )
                .build(),
        ]);
        ast.set_merkle_hash();
        as_migration(ast)
    };

    // Act
    let sql = D1Generator::migrate(
        &m2m_ast,
        Some(&no_m2m_ast),
        &MockMigrationsIntent::default(),
    );

    // Assert
    query(&db, &sql)
        .await
        .expect("Create table queries to work");

    assert!(exists_in_db(&db, "StudentsCourses").await)
}

#[sqlx::test]
async fn views_auto_alias(db: SqlitePool) {
    // Arrange
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
        .data_source(
            "default",
            IncludeTreeBuilder::default()
                .add_with_children("matches", |b| b.add_node("horse2"))
                .build(),
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

    let mut ast = create_ast(vec![horse_model, match_model]);
    ast.set_merkle_hash();

    // Act
    let sql = D1Generator::migrate(&as_migration(ast), None, &MockMigrationsIntent::default());

    // Assert
    expected_str!(
        sql,
        r#"CREATE VIEW IF NOT EXISTS "Horse.default" AS SELECT "Horse"."id" AS "id", "Horse"."name" AS "name", "Horse"."bio" AS "bio", "Match"."id" AS "matches.id", "Match"."horseId1" AS "matches.horseId1", "Match"."horseId2" AS "matches.horseId2", "Horse1"."id" AS "matches.horse2.id", "Horse1"."name" AS "matches.horse2.name", "Horse1"."bio" AS "matches.horse2.bio" FROM "Horse" LEFT JOIN "Match" ON "Horse"."id" = "Match"."horseId1" LEFT JOIN "Horse" AS "Horse1" ON "Match"."horseId2" = "Horse1"."id";"#
    );

    query(&db, &sql).await.expect("create to work");
    sqlx::query("SELECT * FROM [Horse.default]")
        .execute(&db)
        .await
        .expect("Select query to work");
}
