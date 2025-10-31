use std::collections::HashMap;

use common::{
    CidlType, NavigationPropertyKind,
    builder::{IncludeTreeBuilder, ModelBuilder, create_ast},
    err::GeneratorErrorKind,
};

use d1::{D1Generator, MigrationsIntent};
use sqlx::SqlitePool;

macro_rules! expected_str {
    ($got:expr, $expected:expr) => {{
        let got_val = &$got;
        let expected_val = &$expected;
        assert!(
            got_val.to_string().contains(&expected_val.to_string()),
            "Expected: \n`{}`, \n\ngot:\n{:?}",
            expected_val,
            got_val
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

pub async fn query(db: &SqlitePool, sql: &str) -> Result<(), sqlx::Error> {
    let tx = db.begin().await?;
    sqlx::query(sql).execute(db).await?;
    tx.commit().await?;
    Ok(())
}

#[derive(Default)]
struct MockMigrationsIntent {
    answers: HashMap<String, Option<String>>,
}

impl MigrationsIntent for MockMigrationsIntent {
    fn ask(&self, dilemma: d1::MigrationsDilemma) -> Option<usize> {
        match &dilemma {
            d1::MigrationsDilemma::RenameOrDropModel { name, options }
            | d1::MigrationsDilemma::RenameOrDropAttribute { name, options } => {
                let ans = self.answers.get(name).unwrap().clone();
                ans.map(|a| {
                    options
                        .iter()
                        .enumerate()
                        .find(|(_, o)| ***o == a)
                        .unwrap()
                        .0
                })
            }
        }
    }
}

#[sqlx::test]
async fn migrate_models_scalars(db: SqlitePool) {
    let mut empty_ast = create_ast(vec![]);
    empty_ast.set_merkle_hash();

    // Create
    let ast = {
        // Arrange
        let mut ast = create_ast(vec![
            ModelBuilder::new("User")
                .id()
                .attribute("name", CidlType::nullable(CidlType::Text), None)
                .attribute("age", CidlType::Integer, None)
                .attribute("address", CidlType::Text, None)
                .build(),
        ]);

        // Act
        let sql = D1Generator::migrate_ast(
            &mut ast,
            Some(&empty_ast),
            Box::new(MockMigrationsIntent::default()),
        )
        .expect("migrate ast to work");

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
        let sql = D1Generator::migrate_ast(
            &mut empty_ast,
            Some(&ast),
            Box::new(MockMigrationsIntent::default()),
        )
        .expect("migrate ast to work");

        // Assert
        expected_str!(sql, "DROP TABLE IF EXISTS \"User\"");

        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "User").await);
    }
}

#[sqlx::test]
async fn migrate_models_one_to_one(db: SqlitePool) {
    let mut empty_ast = create_ast(vec![]);
    empty_ast.set_merkle_hash();

    // Create
    let ast = {
        // Arrange
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

        // Act
        let sql = D1Generator::migrate_ast(
            &mut ast,
            Some(&empty_ast),
            Box::new(MockMigrationsIntent::default()),
        )
        .expect("migrate ast to work");

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );
        expected_str!(
            sql,
            r#"CREATE VIEW IF NOT EXISTS "Person.default" AS SELECT "Person"."id" AS "Person.id", "Person"."dogId" AS "Person.dogId", "Dog"."id" AS "Person.dog.id" FROM "Person" LEFT JOIN "Dog" ON "Person"."dogId" = "Dog"."id""#
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
        let sql = D1Generator::migrate_ast(
            &mut empty_ast,
            Some(&ast),
            Box::new(MockMigrationsIntent::default()),
        )
        .expect("migrate ast to work");

        // Assert
        query(&db, &sql).await.expect("Drop query to work");

        assert!(!exists_in_db(&db, "Person").await);
        assert!(!exists_in_db(&db, "Dog").await);
        assert!(!exists_in_db(&db, "Person.default").await);
    }
}

#[sqlx::test]
async fn migrate_models_one_to_many(db: SqlitePool) {
    let mut empty_ast = create_ast(vec![]);
    empty_ast.set_merkle_hash();

    // Create
    let ast = {
        // Arrange
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

        // Act
        let sql = D1Generator::migrate_ast(
            &mut ast,
            Some(&empty_ast),
            Box::new(MockMigrationsIntent::default()),
        )
        .expect("migrate ast to work");

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
            r#"CREATE VIEW IF NOT EXISTS "Person.default" AS SELECT "Person"."id" AS "Person.id", "Person"."bossId" AS "Person.bossId", "Cat"."id" AS "Person.cats.id", "Cat"."personId" AS "Person.cats.personId", "Dog"."id" AS "Person.dogs.id", "Dog"."personId" AS "Person.dogs.personId" FROM "Person" LEFT JOIN "Cat" ON "Person"."id" = "Cat"."personId" LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId";"#
        );
        expected_str!(
            sql,
            r#"CREATE VIEW IF NOT EXISTS "Boss.default" AS SELECT "Boss"."id" AS "Boss.id", "Person"."id" AS "Boss.persons.id", "Person"."bossId" AS "Boss.persons.bossId", "Cat"."id" AS "Boss.persons.cats.id", "Cat"."personId" AS "Boss.persons.cats.personId", "Dog"."id" AS "Boss.persons.dogs.id", "Dog"."personId" AS "Boss.persons.dogs.personId" FROM "Boss" LEFT JOIN "Person" ON "Boss"."id" = "Person"."bossId" LEFT JOIN "Cat" ON "Person"."id" = "Cat"."personId" LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId";"#
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
        let sql = D1Generator::migrate_ast(
            &mut empty_ast,
            Some(&ast),
            Box::new(MockMigrationsIntent::default()),
        )
        .expect("migrate ast to work");

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
    let mut empty_ast = create_ast(vec![]);
    empty_ast.set_merkle_hash();

    // Create
    let ast = {
        // Arrange
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

        // Act
        let sql = D1Generator::migrate_ast(
            &mut ast,
            Some(&empty_ast),
            Box::new(MockMigrationsIntent::default()),
        )
        .expect("migrate ast to work");

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
            r#"CREATE VIEW IF NOT EXISTS "Course.withStudents" AS SELECT "Course"."id" AS "Course.id", "StudentsCourses"."Student.id" AS "Course.students.id" FROM "Course" LEFT JOIN "StudentsCourses" ON "Course"."id" = "StudentsCourses"."Course.id" LEFT JOIN "Student" ON "StudentsCourses"."Student.id" = "Student"."id";"#
        );

        expected_str!(
            sql,
            r#"CREATE VIEW IF NOT EXISTS "Student.withCourses" AS SELECT "Student"."id" AS "Student.id", "StudentsCourses"."Course.id" AS "Student.courses.id" FROM "Student" LEFT JOIN "StudentsCourses" ON "Student"."id" = "StudentsCourses"."Student.id" LEFT JOIN "Course" ON "StudentsCourses"."Course.id" = "Course"."id";"#
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
        let sql = D1Generator::migrate_ast(
            &mut empty_ast,
            Some(&ast),
            Box::new(MockMigrationsIntent::default()),
        )
        .expect("migrate ast to work");

        // Assert
        query(&db, &sql).await.expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "StudentsCourses").await);
    }
}

#[sqlx::test]
async fn migrate_with_alterations(db: SqlitePool) {
    let mut base_ast = {
        let mut ast = create_ast(vec![
            ModelBuilder::new("User")
                .id()
                .attribute("name", CidlType::nullable(CidlType::Text), None)
                .attribute("age", CidlType::Integer, None)
                .attribute("address", CidlType::Text, None)
                .build(),
        ]);

        let sql =
            D1Generator::migrate_ast(&mut ast, None, Box::new(MockMigrationsIntent::default()))
                .expect("migrate ast to work");
        query(&db, &sql)
            .await
            .expect("Create table queries to work");

        ast
    };

    // Changes without Rebuild
    base_ast = {
        // Arrange
        let mut new = create_ast(vec![
            ModelBuilder::new("User")
                .id()
                .attribute("first_name", CidlType::nullable(CidlType::Text), None) // changed name
                .attribute("age", CidlType::Text, None) // changed type
                .attribute("favorite_color", CidlType::Text, None) // added column
                // dropped column "address"
                .build(),
        ]);
        new.set_merkle_hash();

        let mut intent = MockMigrationsIntent::default();
        intent
            .answers
            .insert("User.name".into(), Some("first_name".into()));
        intent.answers.insert("User.address".into(), None);

        // Act
        let sql = D1Generator::migrate_ast(&mut new, Some(&base_ast), Box::new(intent))
            .expect("migrate ast to work");

        // Assert
        expected_str!(
            sql,
            "ALTER TABLE \"User\" RENAME COLUMN \"name\" TO \"first_name\""
        );
        expected_str!(
            sql,
            r#"ALTER TABLE "User" DROP COLUMN "age";
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
        let mut new = create_ast(vec![
            ModelBuilder::new("User")
                .id()
                .attribute("first_name", CidlType::nullable(CidlType::Text), None)
                .attribute("age", CidlType::Text, None)
                .attribute("favorite_color", CidlType::Text, None)
                .build(),
        ]);
        new.models[0].primary_key.cidl_type = CidlType::Text; // new PK type
        new.set_merkle_hash();

        // Act
        let sql = D1Generator::migrate_ast(
            &mut new,
            Some(&base_ast),
            Box::new(MockMigrationsIntent::default()),
        )
        .expect("migrate ast to work");

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
        let mut new = create_ast(vec![
            ModelBuilder::new("Dog").id().build(), // added Dog
            ModelBuilder::new("User")
                .id()
                .attribute("first_name", CidlType::nullable(CidlType::Text), None)
                .attribute("age", CidlType::Text, None)
                .attribute("favorite_color", CidlType::Text, None)
                .attribute("dog_id", CidlType::Integer, Some("Dog".into())) // added Dog FK
                .build(),
        ]);
        new.models[1].primary_key.cidl_type = CidlType::Text;
        new.set_merkle_hash();

        // Act
        let sql = D1Generator::migrate_ast(
            &mut new,
            Some(&base_ast),
            Box::new(MockMigrationsIntent::default()),
        )
        .expect("migrate ast to work");

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
    let sql = D1Generator::migrate_ast(&mut ast, None, Box::new(MockMigrationsIntent::default()))
        .expect("migrate ast to work");

    // Assert
    expected_str!(
        sql,
        r#"CREATE VIEW IF NOT EXISTS "Horse.default" AS SELECT "Horse"."id" AS "Horse.id", "Horse"."name" AS "Horse.name", "Horse"."bio" AS "Horse.bio", "Match"."id" AS "Horse.matches.id", "Match"."horseId1" AS "Horse.matches.horseId1", "Match"."horseId2" AS "Horse.matches.horseId2", "Horse1"."id" AS "Horse.matches.horse2.id", "Horse1"."name" AS "Horse.matches.horse2.name", "Horse1"."bio" AS "Horse.matches.horse2.bio" FROM "Horse" LEFT JOIN "Match" ON "Horse"."id" = "Match"."horseId1" LEFT JOIN "Horse" AS "Horse1" ON "Match"."horseId2" = "Horse1"."id";"#
    );

    query(&db, &sql).await.expect("create to work");
    sqlx::query("SELECT * FROM [Horse.default]")
        .execute(&db)
        .await
        .expect("Select query to work");
}

#[test]
fn test_cycle_detection_error() {
    // Arrange
    // A -> B -> C -> A
    let mut ast = create_ast(vec![
        ModelBuilder::new("A")
            .id()
            .attribute("bId", CidlType::Integer, Some("B".to_string()))
            .build(),
        ModelBuilder::new("B")
            .id()
            .attribute("cId", CidlType::Integer, Some("C".to_string()))
            .build(),
        ModelBuilder::new("C")
            .id()
            .attribute("aId", CidlType::Integer, Some("A".to_string()))
            .build(),
    ]);
    ast.set_merkle_hash();

    // Act

    let err = D1Generator::migrate_ast(&mut ast, None, Box::new(MockMigrationsIntent::default()))
        .unwrap_err();

    // Assert
    assert!(matches!(
        err.kind,
        GeneratorErrorKind::CyclicalModelDependency
    ));
    expected_str!(err.context, "A, B, C");
}

#[test]
fn test_nullability_prevents_cycle_error() {
    // Arrange
    // A -> B -> C -> Nullable<A>
    let mut ast = create_ast(vec![
        ModelBuilder::new("A")
            .id()
            .attribute("bId", CidlType::Integer, Some("B".to_string()))
            .build(),
        ModelBuilder::new("B")
            .id()
            .attribute("cId", CidlType::Integer, Some("C".to_string()))
            .build(),
        ModelBuilder::new("C")
            .id()
            .attribute(
                "aId",
                CidlType::nullable(CidlType::Integer),
                Some("A".to_string()),
            )
            .build(),
    ]);

    // Act

    // Assert
    D1Generator::migrate_ast(&mut ast, None, Box::new(MockMigrationsIntent::default()))
        .expect("migrate ast to work");
}

#[test]
fn test_one_to_one_nav_property_unknown_attribute_reference_error() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Dog").id().build(),
        ModelBuilder::new("Person")
            .id()
            .nav_p(
                "dog",
                "Dog",
                NavigationPropertyKind::OneToOne {
                    reference: "dogId".to_string(),
                },
            )
            .build(),
    ]);

    // Act
    let err = D1Generator::migrate_ast(&mut ast, None, Box::new(MockMigrationsIntent::default()))
        .unwrap_err();

    // Assert
    assert!(matches!(
        err.kind,
        GeneratorErrorKind::InvalidNavigationPropertyReference
    ));
}

#[test]
fn test_one_to_one_mismatched_fk_and_nav_type_error() {
    // Arrange: attribute dogId references Dog, but nav prop type is Cat -> mismatch
    let mut ast = create_ast(vec![
        ModelBuilder::new("Dog").id().build(),
        ModelBuilder::new("Cat").id().build(),
        ModelBuilder::new("Person")
            .id()
            .attribute("dogId", CidlType::Integer, Some("Dog".into()))
            .nav_p(
                "dog",
                "Cat", // incorrect: says Cat but fk points to Dog
                NavigationPropertyKind::OneToOne {
                    reference: "dogId".to_string(),
                },
            )
            .build(),
    ]);

    // Act
    let err = D1Generator::migrate_ast(&mut ast, None, Box::new(MockMigrationsIntent::default()))
        .unwrap_err();

    // Assert
    assert!(matches!(
        err.kind,
        GeneratorErrorKind::MismatchedNavigationPropertyTypes
    ));
}

#[test]
fn test_one_to_many_unresolved_reference_error() {
    // Arrange:
    // Person declares OneToMany to Dog referencing Dog.personId, but Dog has no personId FK attr.
    let mut ast = create_ast(vec![
        ModelBuilder::new("Dog").id().build(), // no personId attribute
        ModelBuilder::new("Person")
            .id()
            .nav_p(
                "dogs",
                "Dog",
                NavigationPropertyKind::OneToMany {
                    reference: "personId".into(),
                },
            )
            .build(),
    ]);

    // Act
    let err = D1Generator::migrate_ast(&mut ast, None, Box::new(MockMigrationsIntent::default()))
        .unwrap_err();

    // Assert
    expected_str!(
        err.context,
        "Person.dogs references Dog.personId which does not exist or is not a foreign key to Person"
    );
}

#[test]
fn test_junction_table_builder_errors() {
    // Missing second nav property case: only one side of many-to-many
    {
        let mut ast = create_ast(vec![
            ModelBuilder::new("Student")
                .id()
                .nav_p(
                    "courses",
                    "Course",
                    NavigationPropertyKind::ManyToMany {
                        unique_id: "OnlyOne".into(),
                    },
                )
                .build(),
            // Course exists, but doesn't declare the reciprocal nav property
            ModelBuilder::new("Course").id().build(),
        ]);

        let err =
            D1Generator::migrate_ast(&mut ast, None, Box::new(MockMigrationsIntent::default()))
                .unwrap_err();
        assert!(matches!(
            err.kind,
            GeneratorErrorKind::MissingManyToManyReference
        ));
    }

    // Too many models case: three models register the same junction id
    {
        let mut ast = create_ast(vec![
            ModelBuilder::new("A")
                .id()
                .nav_p(
                    "bs",
                    "B",
                    NavigationPropertyKind::ManyToMany {
                        unique_id: "TriJ".into(),
                    },
                )
                .build(),
            ModelBuilder::new("B")
                .id()
                .nav_p(
                    "as",
                    "A",
                    NavigationPropertyKind::ManyToMany {
                        unique_id: "TriJ".into(),
                    },
                )
                .build(),
            // Third model C tries to use the same junction id -> should error
            ModelBuilder::new("C")
                .id()
                .nav_p(
                    "as",
                    "A",
                    NavigationPropertyKind::ManyToMany {
                        unique_id: "TriJ".into(),
                    },
                )
                .build(),
        ]);

        let err =
            D1Generator::migrate_ast(&mut ast, None, Box::new(MockMigrationsIntent::default()))
                .unwrap_err();
        assert!(matches!(
            err.kind,
            GeneratorErrorKind::ExtraneousManyToManyReferences
        ));
    }
}
