use common::{
    CidlType, NavigationPropertyKind,
    builder::{IncludeTreeBuilder, ModelBuilder, create_ast},
};

use d1::D1Generator;
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

#[sqlx::test]
async fn migrate_models_scalars(db: SqlitePool) {
    let mut empty_ast = create_ast(vec![]);
    empty_ast.set_merkle_hash();

    // Insert
    let ast = {
        // Arrange
        let mut ast = create_ast(vec![
            ModelBuilder::new("User")
                .id() // adds a primary key
                .attribute("name", CidlType::nullable(CidlType::Text), None)
                .attribute("age", CidlType::Integer, None)
                .build(),
        ]);

        // Act
        let sql =
            D1Generator::migrate_ast(&mut ast, Some(&empty_ast)).expect("generate_sql to work");

        // Assert
        expected_str!(sql, "CREATE TABLE IF NOT EXISTS");
        expected_str!(sql, "\"id\" integer PRIMARY KEY");
        expected_str!(sql, "\"name\" text");
        expected_str!(sql, "\"age\" integer NOT NULL");

        sqlx::query(&sql)
            .execute(&db)
            .await
            .expect("Insert table query to work");
        assert!(exists_in_db(&db, "User").await);

        ast
    };

    // Drop
    {
        // Act
        let sql =
            D1Generator::migrate_ast(&mut empty_ast, Some(&ast)).expect("generate_sql to work");

        // Assert
        expected_str!(sql, "DROP TABLE IF EXISTS \"User\"");

        sqlx::query(&sql)
            .execute(&db)
            .await
            .expect("Drop tables query to work");
        assert!(!exists_in_db(&db, "User").await);
    }
}

#[sqlx::test]
async fn migrate_models_one_to_one(db: SqlitePool) {
    let mut empty_ast = create_ast(vec![]);

    // Insert
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
        let sql =
            D1Generator::migrate_ast(&mut ast, Some(&empty_ast)).expect("generate_sql to work");

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );
        expected_str!(
            sql,
            r#"CREATE VIEW IF NOT EXISTS "Person.default" AS SELECT "Person"."id" AS "Person.id", "Person"."dogId" AS "Person.dogId", "Dog"."id" AS "Person.dog.id" FROM "Person" LEFT JOIN "Dog" ON "Person"."dogId" = "Dog"."id""#
        );

        sqlx::query(&sql)
            .execute(&db)
            .await
            .expect("Insert query to work");
        assert!(exists_in_db(&db, "Person").await);
        assert!(exists_in_db(&db, "Dog").await);
        assert!(exists_in_db(&db, "Person.default").await);

        ast
    };

    // Drop
    {
        // Act
        let sql =
            D1Generator::migrate_ast(&mut empty_ast, Some(&ast)).expect("generate_sql to work");

        // Assert
        sqlx::query(&sql)
            .execute(&db)
            .await
            .expect("Drop query to work");
        assert!(!exists_in_db(&db, "Person").await);
        assert!(!exists_in_db(&db, "Dog").await);
        assert!(!exists_in_db(&db, "Person.default").await);
    }
}

// #[test]
// fn test_sqlite_table_output() {

//     // One to Many
//     {
//         // Arrange
//         let ast = create_ast(vec![
//             ModelBuilder::new("Dog")
//                 .id()
//                 .attribute("personId", CidlType::Integer, Some("Person".into()))
//                 .build(),
//             ModelBuilder::new("Cat")
//                 .attribute("personId", CidlType::Integer, Some("Person".into()))
//                 .id()
//                 .build(),
//             ModelBuilder::new("Person")
//                 .id()
//                 .nav_p(
//                     "dogs",
//                     "Dog",
//                     NavigationPropertyKind::OneToMany {
//                         reference: "personId".into(),
//                     },
//                 )
//                 .nav_p(
//                     "cats",
//                     "Cat",
//                     NavigationPropertyKind::OneToMany {
//                         reference: "personId".into(),
//                     },
//                 )
//                 .attribute("bossId", CidlType::Integer, Some("Boss".into()))
//                 .build(),
//             ModelBuilder::new("Boss")
//                 .id()
//                 .nav_p(
//                     "persons",
//                     "Person",
//                     NavigationPropertyKind::OneToMany {
//                         reference: "bossId".into(),
//                     },
//                 )
//                 .build(),
//         ]);

//         // Act
//         let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");

//         // Assert: boss table
//         expected_str!(sql, r#"CREATE TABLE "Boss" ( "id" integer PRIMARY KEY );"#);

//         // Assert: person table with FK to boss
//         expected_str!(
//             sql,
//             r#"CREATE TABLE "Person" ( "id" integer PRIMARY KEY, "bossId" integer NOT NULL, FOREIGN KEY ("bossId") REFERENCES "Boss" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
//         );

//         // Assert: dog table with FK to person
//         expected_str!(
//             sql,
//             r#"CREATE TABLE "Dog" ( "id" integer PRIMARY KEY, "personId" integer NOT NULL, FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
//         );

//         // Assert: cat table with FK to person
//         expected_str!(
//             sql,
//             r#"CREATE TABLE "Cat" ( "id" integer PRIMARY KEY, "personId" integer NOT NULL, FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
//         );
//     }

//     // Many to Many
//     {
//         // Arrange
//         let ast = create_ast(vec![
//             ModelBuilder::new("Student")
//                 .id()
//                 .nav_p(
//                     "courses",
//                     "Course",
//                     NavigationPropertyKind::ManyToMany {
//                         unique_id: "StudentsCourses".into(),
//                     },
//                 )
//                 .build(),
//             ModelBuilder::new("Course")
//                 .id()
//                 .nav_p(
//                     "students",
//                     "Student",
//                     NavigationPropertyKind::ManyToMany {
//                         unique_id: "StudentsCourses".into(),
//                     },
//                 )
//                 .build(),
//         ]);

//         // Act
//         let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");

//         // Assert: Junction table exists
//         expected_str!(sql, r#"CREATE TABLE "StudentsCourses""#);

//         // Assert: Junction table has StudentId + CourseId composite PK
//         expected_str!(sql, r#""Student.id" integer NOT NULL"#);
//         expected_str!(sql, r#""Course.id" integer NOT NULL"#);
//         expected_str!(sql, r#"PRIMARY KEY ("Course.id", "Student.id")"#);

//         // Assert: FKs to Student and Course
//         expected_str!(
//             sql,
//             r#"FOREIGN KEY ("Student.id") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
//         );
//         expected_str!(
//             sql,
//             r#"FOREIGN KEY ("Course.id") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
//         );
//     }
// }

// #[test]
// fn test_sqlite_view_output() {
//     // One to One
//     {
//         // Arrange
//         let ast = create_ast(vec![
//             ModelBuilder::new("Person")
//                 .id()
//                 .attribute("dogId", CidlType::Integer, Some("Dog".into()))
//                 .nav_p(
//                     "dog",
//                     "Dog",
//                     NavigationPropertyKind::OneToOne {
//                         reference: "dogId".into(),
//                     },
//                 )
//                 // Data Source includes Dog nav prop
//                 .data_source(
//                     "default",
//                     IncludeTreeBuilder::default().add_node("dog").build(),
//                 )
//                 .build(),
//             ModelBuilder::new("Dog").id().build(),
//         ]);

//         // Act
//         let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");

//         // Assert
//         expected_str!(
//             sql,
//             r#"CREATE VIEW "Person.default" AS SELECT "Person"."id" AS "Person.id", "Person"."dogId" AS "Person.dogId", "Dog"."id" AS "Person.dog.id" FROM "Person" LEFT JOIN "Dog" ON "Person"."dogId" = "Dog"."id""#
//         )
//     }

//     // One to Many
//     {
//         // Arrange
//         let ast = create_ast(vec![
//             ModelBuilder::new("Dog")
//                 .id()
//                 .attribute("personId", CidlType::Integer, Some("Person".into()))
//                 .build(),
//             ModelBuilder::new("Cat")
//                 .attribute("personId", CidlType::Integer, Some("Person".into()))
//                 .id()
//                 .build(),
//             ModelBuilder::new("Person")
//                 .id()
//                 .nav_p(
//                     "dogs",
//                     "Dog",
//                     NavigationPropertyKind::OneToMany {
//                         reference: "personId".into(),
//                     },
//                 )
//                 .nav_p(
//                     "cats",
//                     "Cat",
//                     NavigationPropertyKind::OneToMany {
//                         reference: "personId".into(),
//                     },
//                 )
//                 .attribute("bossId", CidlType::Integer, Some("Boss".into()))
//                 .data_source(
//                     "default",
//                     IncludeTreeBuilder::default()
//                         .add_node("dogs")
//                         .add_node("cats")
//                         .build(),
//                 )
//                 .build(),
//             ModelBuilder::new("Boss")
//                 .id()
//                 .nav_p(
//                     "persons",
//                     "Person",
//                     NavigationPropertyKind::OneToMany {
//                         reference: "bossId".into(),
//                     },
//                 )
//                 .data_source(
//                     "default",
//                     IncludeTreeBuilder::default()
//                         .add_with_children("persons", |b| b.add_node("dogs").add_node("cats"))
//                         .build(),
//                 )
//                 .build(),
//         ]);

//         // Act
//         let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");

//         // Assert
//         expected_str!(
//             sql,
//             r#"CREATE VIEW "Person.default" AS SELECT "Person"."id" AS "Person.id", "Person"."bossId" AS "Person.bossId", "Cat"."id" AS "Person.cats.id", "Cat"."personId" AS "Person.cats.personId", "Dog"."id" AS "Person.dogs.id", "Dog"."personId" AS "Person.dogs.personId" FROM "Person" LEFT JOIN "Cat" ON "Person"."id" = "Cat"."personId" LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId";"#
//         );

//         expected_str!(
//             sql,
//             r#"CREATE VIEW "Boss.default" AS SELECT "Boss"."id" AS "Boss.id", "Person"."id" AS "Boss.persons.id", "Person"."bossId" AS "Boss.persons.bossId", "Cat"."id" AS "Boss.persons.cats.id", "Cat"."personId" AS "Boss.persons.cats.personId", "Dog"."id" AS "Boss.persons.dogs.id", "Dog"."personId" AS "Boss.persons.dogs.personId" FROM "Boss" LEFT JOIN "Person" ON "Boss"."id" = "Person"."bossId" LEFT JOIN "Cat" ON "Person"."id" = "Cat"."personId" LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId";"#
//         );
//     }

//     // Many to Many
//     {
//         // Arrange
//         let ast = create_ast(vec![
//             ModelBuilder::new("Student")
//                 .id()
//                 .nav_p(
//                     "courses",
//                     "Course".to_string(),
//                     NavigationPropertyKind::ManyToMany {
//                         unique_id: "StudentsCourses".into(),
//                     },
//                 )
//                 .data_source(
//                     "withCourses",
//                     IncludeTreeBuilder::default().add_node("courses").build(),
//                 )
//                 .build(),
//             ModelBuilder::new("Course")
//                 .id()
//                 .nav_p(
//                     "students",
//                     "Student".to_string(),
//                     NavigationPropertyKind::ManyToMany {
//                         unique_id: "StudentsCourses".into(),
//                     },
//                 )
//                 .data_source(
//                     "withStudents",
//                     IncludeTreeBuilder::default().add_node("students").build(),
//                 )
//                 .build(),
//         ]);

//         // Act
//         let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");
//         expected_str!(sql, r#"CREATE TABLE "StudentsCourses""#);

//         // Assert: Many-to-many view for Student
//         expected_str!(
//             sql,
//             r#"CREATE VIEW "Course.withStudents" AS SELECT "Course"."id" AS "Course.id", "StudentsCourses"."Student.id" AS "Course.students.id" FROM "Course" LEFT JOIN "StudentsCourses" ON "Course"."id" = "StudentsCourses"."Course.id" LEFT JOIN "Student" ON "StudentsCourses"."Student.id" = "Student"."id";"#
//         );

//         // Assert: Many-to-many view for Course
//         expected_str!(
//             sql,
//             r#"CREATE VIEW "Student.withCourses" AS SELECT "Student"."id" AS "Student.id", "StudentsCourses"."Course.id" AS "Student.courses.id" FROM "Student" LEFT JOIN "StudentsCourses" ON "Student"."id" = "StudentsCourses"."Student.id" LEFT JOIN "Course" ON "StudentsCourses"."Course.id" = "Course"."id";"#
//         );
//     }

//     // Auto aliasing
//     {
//         // Arrange
//         let horse_model = ModelBuilder::new("Horse")
//             // Attributes
//             .id() // id is primary key
//             .attribute("name", CidlType::Text, None)
//             .attribute("bio", CidlType::nullable(CidlType::Text), None)
//             // Navigation Properties
//             .nav_p(
//                 "matches",
//                 "Match",
//                 NavigationPropertyKind::OneToMany {
//                     reference: "horseId1".into(),
//                 },
//             )
//             // Data Sources
//             .data_source(
//                 "default",
//                 IncludeTreeBuilder::default()
//                     .add_with_children("matches", |b| b.add_node("horse2"))
//                     .build(),
//             )
//             .build();

//         let match_model = ModelBuilder::new("Match")
//             // Attributes
//             .id()
//             .attribute("horseId1", CidlType::Integer, Some("Horse".into()))
//             .attribute("horseId2", CidlType::Integer, Some("Horse".into()))
//             // Navigation Properties
//             .nav_p(
//                 "horse2",
//                 "Horse",
//                 NavigationPropertyKind::OneToOne {
//                     reference: "horseId2".into(),
//                 },
//             )
//             .build();

//         let ast = create_ast(vec![horse_model, match_model]);

//         // Act
//         let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");

//         // Assert
//         expected_str!(
//             sql,
//             r#"CREATE VIEW "Horse.default" AS SELECT "Horse"."id" AS "Horse.id", "Horse"."name" AS "Horse.name", "Horse"."bio" AS "Horse.bio", "Match"."id" AS "Horse.matches.id", "Match"."horseId1" AS "Horse.matches.horseId1", "Match"."horseId2" AS "Horse.matches.horseId2", "Horse1"."id" AS "Horse.matches.horse2.id", "Horse1"."name" AS "Horse.matches.horse2.name", "Horse1"."bio" AS "Horse.matches.horse2.bio" FROM "Horse" LEFT JOIN "Match" ON "Horse"."id" = "Match"."horseId1" LEFT JOIN "Horse" AS "Horse1" ON "Match"."horseId2" = "Horse1"."id";"#
//         );
//     }
// }

// #[test]
// fn test_cycle_detection_error() {
//     // Arrange
//     // A -> B -> C -> A
//     let ast = create_ast(vec![
//         ModelBuilder::new("A")
//             .id()
//             .attribute("bId", CidlType::Integer, Some("B".to_string()))
//             .build(),
//         ModelBuilder::new("B")
//             .id()
//             .attribute("cId", CidlType::Integer, Some("C".to_string()))
//             .build(),
//         ModelBuilder::new("C")
//             .id()
//             .attribute("aId", CidlType::Integer, Some("A".to_string()))
//             .build(),
//     ]);

//     // Act

//     let err = d1::generate_sql(&ast.models).unwrap_err();

//     // Assert
//     assert!(matches!(
//         err.kind,
//         GeneratorErrorKind::CyclicalModelDependency
//     ));
//     expected_str!(err.context, "A, B, C");
// }

// #[test]
// fn test_nullability_prevents_cycle_error() {
//     // Arrange
//     // A -> B -> C -> Nullable<A>
//     let ast = create_ast(vec![
//         ModelBuilder::new("A")
//             .id()
//             .attribute("bId", CidlType::Integer, Some("B".to_string()))
//             .build(),
//         ModelBuilder::new("B")
//             .id()
//             .attribute("cId", CidlType::Integer, Some("C".to_string()))
//             .build(),
//         ModelBuilder::new("C")
//             .id()
//             .attribute(
//                 "aId",
//                 CidlType::nullable(CidlType::Integer),
//                 Some("A".to_string()),
//             )
//             .build(),
//     ]);

//     // Act

//     // Assert
//     d1::generate_sql(&ast.models).expect("sqlite gen to work");
// }

// #[test]
// fn test_one_to_one_nav_property_unknown_attribute_reference_error() {
//     // Arrange
//     let ast = create_ast(vec![
//         ModelBuilder::new("Dog").id().build(),
//         ModelBuilder::new("Person")
//             .id()
//             .nav_p(
//                 "dog",
//                 "Dog",
//                 NavigationPropertyKind::OneToOne {
//                     reference: "dogId".to_string(),
//                 },
//             )
//             .build(),
//     ]);

//     // Act
//     let err = d1::generate_sql(&ast.models).unwrap_err();

//     // Assert
//     assert!(matches!(
//         err.kind,
//         GeneratorErrorKind::InvalidNavigationPropertyReference
//     ))
// }

// #[test]
// fn test_one_to_one_mismatched_fk_and_nav_type_error() {
//     // Arrange: attribute dogId references Dog, but nav prop type is Cat -> mismatch
//     let ast = create_ast(vec![
//         ModelBuilder::new("Dog").id().build(),
//         ModelBuilder::new("Cat").id().build(),
//         ModelBuilder::new("Person")
//             .id()
//             .attribute("dogId", CidlType::Integer, Some("Dog".into()))
//             .nav_p(
//                 "dog",
//                 "Cat", // incorrect: says Cat but fk points to Dog
//                 NavigationPropertyKind::OneToOne {
//                     reference: "dogId".to_string(),
//                 },
//             )
//             .build(),
//     ]);

//     // Act
//     let err = d1::generate_sql(&ast.models).unwrap_err();

//     // Assert
//     assert!(matches!(
//         err.kind,
//         GeneratorErrorKind::MismatchedNavigationPropertyTypes
//     ))
// }

// #[test]
// fn test_one_to_many_unresolved_reference_error() {
//     // Arrange:
//     // Person declares OneToMany to Dog referencing Dog.personId, but Dog has no personId FK attr.
//     let ast = create_ast(vec![
//         ModelBuilder::new("Dog").id().build(), // no personId attribute
//         ModelBuilder::new("Person")
//             .id()
//             .nav_p(
//                 "dogs",
//                 "Dog",
//                 NavigationPropertyKind::OneToMany {
//                     reference: "personId".into(),
//                 },
//             )
//             .build(),
//     ]);

//     // Act
//     let err = d1::generate_sql(&ast.models).unwrap_err();

//     // Assert
//     expected_str!(
//         err.context,
//         "Person.dogs references Dog.personId which does not exist or is not a foreign key to Person"
//     );
// }

// #[test]
// fn test_junction_table_builder_errors() {
//     // Missing second nav property case: only one side of many-to-many
//     {
//         let ast = create_ast(vec![
//             ModelBuilder::new("Student")
//                 .id()
//                 .nav_p(
//                     "courses",
//                     "Course",
//                     NavigationPropertyKind::ManyToMany {
//                         unique_id: "OnlyOne".into(),
//                     },
//                 )
//                 .build(),
//             // Course exists, but doesn't declare the reciprocal nav property
//             ModelBuilder::new("Course").id().build(),
//         ]);

//         let err = d1::generate_sql(&ast.models).unwrap_err();
//         assert!(matches!(
//             err.kind,
//             GeneratorErrorKind::MissingManyToManyReference
//         ))
//     }

//     // Too many models case: three models register the same junction id
//     {
//         let ast = create_ast(vec![
//             ModelBuilder::new("A")
//                 .id()
//                 .nav_p(
//                     "bs",
//                     "B",
//                     NavigationPropertyKind::ManyToMany {
//                         unique_id: "TriJ".into(),
//                     },
//                 )
//                 .build(),
//             ModelBuilder::new("B")
//                 .id()
//                 .nav_p(
//                     "as",
//                     "A",
//                     NavigationPropertyKind::ManyToMany {
//                         unique_id: "TriJ".into(),
//                     },
//                 )
//                 .build(),
//             // Third model C tries to use the same junction id -> should error
//             ModelBuilder::new("C")
//                 .id()
//                 .nav_p(
//                     "as",
//                     "A",
//                     NavigationPropertyKind::ManyToMany {
//                         unique_id: "TriJ".into(),
//                     },
//                 )
//                 .build(),
//         ]);

//         let err = d1::generate_sql(&ast.models).unwrap_err();
//         assert!(matches!(
//             err.kind,
//             GeneratorErrorKind::ExtraneousManyToManyReferences
//         ))
//     }
// }
