use common::{
    CidlType, NavigationPropertyKind,
    builder::{ModelBuilder, create_ast},
};

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

#[test]
fn test_sqlite_table_output() {
    // Empty
    {
        // Arrange
        let ast = create_ast(vec![]);

        // Act
        let sql = d1::generate_sql(&ast.models).expect("Empty models should succeed");

        // Assert
        assert!(
            sql.trim().is_empty(),
            "Expected empty SQL output for empty CIDL, got: {}",
            sql
        );
    }

    // Primary key, Basic attributes
    {
        // Arrange
        let ast = create_ast(vec![
            ModelBuilder::new("User")
                .id() // adds a primary key
                .attribute("name", CidlType::nullable(CidlType::Text), None)
                .attribute("age", CidlType::Integer, None)
                .build(),
        ]);

        // Act
        let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");

        // Assert
        expected_str!(sql, "CREATE TABLE");
        expected_str!(sql, "\"id\" integer PRIMARY KEY");
        expected_str!(sql, "\"name\" text");
        expected_str!(sql, "\"age\" integer NOT NULL");
    }

    // One to One FK's
    {
        // Arrange
        let ast = create_ast(vec![
            ModelBuilder::new("Person")
                .id()
                .attribute("dogId", CidlType::Integer, Some("Dog".to_string()))
                .build(),
            ModelBuilder::new("Dog").id().build(),
        ]);

        // Act
        let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );
    }

    // One to One FK's with Nav Prop
    {
        // Arrange
        let ast = create_ast(vec![
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
        ]);

        // Act
        let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );
    }

    // One to Many
    {
        // Arrange
        let ast = create_ast(vec![
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
        ]);

        // Act
        let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");

        // Assert: boss table
        expected_str!(sql, r#"CREATE TABLE "Boss" ( "id" integer PRIMARY KEY );"#);

        // Assert: person table with FK to boss
        expected_str!(
            sql,
            r#"CREATE TABLE "Person" ( "id" integer PRIMARY KEY, "bossId" integer NOT NULL, FOREIGN KEY ("bossId") REFERENCES "Boss" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
        );

        // Assert: dog table with FK to person
        expected_str!(
            sql,
            r#"CREATE TABLE "Dog" ( "id" integer PRIMARY KEY, "personId" integer NOT NULL, FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
        );

        // Assert: cat table with FK to person
        expected_str!(
            sql,
            r#"CREATE TABLE "Cat" ( "id" integer PRIMARY KEY, "personId" integer NOT NULL, FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
        );
    }

    // Many to Many
    {
        // Arrange
        let ast = create_ast(vec![
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

        // Act
        let sql = d1::generate_sql(&ast.models).expect("gen_sqlite to work");

        // Assert: Junction table exists
        expected_str!(sql, r#"CREATE TABLE "StudentsCourses""#);

        // Assert: Junction table has StudentId + CourseId composite PK
        expected_str!(sql, r#""Student_id" integer NOT NULL"#);
        expected_str!(sql, r#""Course_id" integer NOT NULL"#);
        expected_str!(sql, r#"PRIMARY KEY ("Course_id", "Student_id")"#);

        // Assert: FKs to Student and Course
        expected_str!(
            sql,
            r#"FOREIGN KEY ("Student_id") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
        );
        expected_str!(
            sql,
            r#"FOREIGN KEY ("Course_id") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE"#
        );
    }
}

#[test]
fn test_cycle_detection_error() {
    // Arrange
    // A -> B -> C -> A
    let ast = create_ast(vec![
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

    // Act

    let err = d1::generate_sql(&ast.models).unwrap_err();

    // Assert
    expected_str!(err, "Cycle detected");
}

#[test]
fn test_nullability_prevents_cycle_error() {
    // Arrange
    // A -> B -> C -> Nullable<A>
    let ast = create_ast(vec![
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
    d1::generate_sql(&ast.models).expect("sqlite gen to work");
}

#[test]
fn test_one_to_one_nav_property_unknown_attribute_reference_error() {
    // Arrange
    let ast = create_ast(vec![
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
    let err = d1::generate_sql(&ast.models).unwrap_err();

    // Assert
    expected_str!(
        err,
        "Navigation property Person.dog references Dog.dogId which does not exist."
    );
}

#[test]
fn test_one_to_one_mismatched_fk_and_nav_type_error() {
    // Arrange: attribute dogId references Dog, but nav prop type is Cat -> mismatch
    let ast = create_ast(vec![
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
    let err = d1::generate_sql(&ast.models).unwrap_err();

    // Assert - message includes "Mismatched types between foreign key and One to One navigation property"
    expected_str!(
        err,
        "Mismatched types between foreign key and One to One navigation property"
    );
}

#[test]
fn test_one_to_many_unresolved_reference_error() {
    // Arrange:
    // Person declares OneToMany to Dog referencing Dog.personId, but Dog has no personId FK attr.
    let ast = create_ast(vec![
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
    let err = d1::generate_sql(&ast.models).unwrap_err();

    // Assert
    expected_str!(
        err,
        "Navigation property Person.dogs references Dog.personId which does not exist."
    );
}

#[test]
fn test_junction_table_builder_errors() {
    // Missing second nav property case: only one side of many-to-many
    {
        let ast = create_ast(vec![
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

        let err = d1::generate_sql(&ast.models).unwrap_err();
        expected_str!(err, "Both models must be set for a junction table");
    }

    // Too many models case: three models register the same junction id
    {
        let ast = create_ast(vec![
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

        let err = d1::generate_sql(&ast.models).unwrap_err();
        expected_str!(
            err,
            "Too many ManyToMany navigation properties for junction table"
        );
    }
}
