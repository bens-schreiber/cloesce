use common::{
    CidlForeignKeyKind, CidlType,
    builder::{IncludeTreeBuilder, ModelBuilder, create_cidl, create_wrangler},
};
use d1::D1Generator;

macro_rules! expected_str {
    ($got:expr, $expected:expr) => {{
        let got_val = &$got;
        let expected_val = &$expected;
        assert!(
            got_val.to_string().contains(&expected_val.to_string()),
            "Expected `{}`, got:\n{:?}",
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
        let cidl = create_cidl(vec![]);
        let d1gen = D1Generator::new(cidl, create_wrangler());

        // Act
        let sql = d1gen.sql().expect("Empty models should succeed");

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
        let cidl = create_cidl(vec![
            ModelBuilder::new("User")
                .id() // adds a primary key
                .attribute("name", CidlType::Text, true, None)
                .attribute("age", CidlType::Integer, false, None)
                .build(),
        ]);
        let d1gen = D1Generator::new(cidl, create_wrangler());

        // Act
        let sql = d1gen.sql().expect("gen_sqlite to work");

        // Assert
        expected_str!(sql, "CREATE TABLE");
        expected_str!(sql, "\"id\" integer PRIMARY KEY");
        expected_str!(sql, "\"name\" text");
        expected_str!(sql, "\"age\" integer NOT NULL");
    }

    // One to One FK's
    {
        // Arrange
        let cidl = create_cidl(vec![
            ModelBuilder::new("Person")
                .id()
                .attribute("dogId", CidlType::Integer, false, Some("Dog".to_string()))
                .build(),
            ModelBuilder::new("Dog").id().build(),
        ]);
        let d1gen = D1Generator::new(cidl, create_wrangler());

        // Act
        let sql = d1gen.sql().expect("gen_sqlite to work");

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );
    }

    // One to One FK's with Nav Prop
    {
        // Arrange
        let cidl = create_cidl(vec![
            ModelBuilder::new("Person")
                .id()
                .attribute("dogId", CidlType::Integer, false, Some("Dog".into()))
                .nav_p(
                    "dog",
                    CidlType::Model("Dog".into()),
                    false,
                    CidlForeignKeyKind::OneToOne {
                        reference: "dogId".into(),
                    },
                )
                .build(),
            ModelBuilder::new("Dog").id().build(),
        ]);
        let d1gen = D1Generator::new(cidl, create_wrangler());

        // Act
        let sql = d1gen.sql().expect("gen_sqlite to work");

        // Assert
        expected_str!(
            sql,
            r#"FOREIGN KEY ("dogId") REFERENCES "Dog" ("id") ON DELETE RESTRICT ON UPDATE CASCADE "#
        );
    }

    // One to Many
    {
        // Arrange
        let cidl = create_cidl(vec![
            ModelBuilder::new("Dog")
                .id()
                .attribute("personId", CidlType::Integer, false, Some("Person".into()))
                .build(),
            ModelBuilder::new("Cat")
                .attribute("personId", CidlType::Integer, false, Some("Person".into()))
                .id()
                .build(),
            ModelBuilder::new("Person")
                .id()
                .nav_p(
                    "dogs",
                    CidlType::Array(Box::new(CidlType::Model("Dog".to_string()))),
                    false,
                    CidlForeignKeyKind::OneToMany {
                        reference: "personId".into(),
                    },
                )
                .nav_p(
                    "cats",
                    CidlType::Array(Box::new(CidlType::Model("Cat".to_string()))),
                    false,
                    CidlForeignKeyKind::OneToMany {
                        reference: "personId".into(),
                    },
                )
                .attribute("bossId", CidlType::Integer, false, Some("Boss".into()))
                .build(),
            ModelBuilder::new("Boss")
                .id()
                .nav_p(
                    "persons",
                    CidlType::Array(Box::new(CidlType::Model("Person".to_string()))),
                    false,
                    CidlForeignKeyKind::OneToMany {
                        reference: "bossId".into(),
                    },
                )
                .build(),
        ]);
        let d1gen = D1Generator::new(cidl, create_wrangler());

        // Act
        let sql = d1gen.sql().expect("gen_sqlite to work");

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
            r#"CREATE TABLE "Cat" ( "personId" integer NOT NULL, "id" integer PRIMARY KEY, FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );"#
        );
    }

    // Many to Many
    {
        // Arrange
        let cidl = create_cidl(vec![
            ModelBuilder::new("Student")
                .id()
                .nav_p(
                    "courses",
                    CidlType::Array(Box::new(CidlType::Model("Course".to_string()))),
                    false,
                    CidlForeignKeyKind::ManyToMany {
                        unique_id: "StudentsCourses".into(),
                    },
                )
                .build(),
            ModelBuilder::new("Course")
                .id()
                .nav_p(
                    "students",
                    CidlType::Array(Box::new(CidlType::Model("Student".to_string()))),
                    false,
                    CidlForeignKeyKind::ManyToMany {
                        unique_id: "StudentsCourses".into(),
                    },
                )
                .build(),
        ]);
        let d1gen = D1Generator::new(cidl, create_wrangler());

        // Act
        let sql = d1gen.sql().expect("gen_sqlite to work");

        // Assert: Junction table exists
        expected_str!(sql, r#"CREATE TABLE "StudentsCourses""#);

        // Assert: Junction table has StudentId + CourseId composite PK
        expected_str!(sql, r#""Student_id" integer NOT NULL"#);
        expected_str!(sql, r#""Course_id" integer NOT NULL"#);
        expected_str!(sql, r#"PRIMARY KEY ("Student_id", "Course_id")"#);

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
fn test_sqlite_view_output() {
    // One to One
    {
        // Arrange
        let cidl = create_cidl(vec![
            ModelBuilder::new("Person")
                .id()
                .attribute("dogId", CidlType::Integer, false, Some("Dog".into()))
                .nav_p(
                    "dog",
                    CidlType::Model("Dog".into()),
                    false,
                    CidlForeignKeyKind::OneToOne {
                        reference: "dogId".into(),
                    },
                )
                // Data Source includes Dog nav prop
                .data_source(
                    "default",
                    IncludeTreeBuilder::default()
                        .add("dog", CidlType::Model("Dog".into()))
                        .build(),
                )
                .build(),
            ModelBuilder::new("Dog").id().build(),
        ]);
        let d1gen = D1Generator::new(cidl, create_wrangler());

        // Act
        let sql = d1gen.sql().expect("gen_sqlite to work");

        // Assert
        expected_str!(
            sql,
            r#"CREATE VIEW "Person_default" AS SELECT "Person"."id" AS "Person_id", "Person"."dogId" AS "Person_dogId", "Dog"."id" AS "Dog_id" FROM "Person" LEFT JOIN "Dog" ON "Person"."dogId" = "Dog"."id""#
        )
    }

    // One to Many
    {
        // Arrange
        let cidl = create_cidl(vec![
            ModelBuilder::new("Dog")
                .id()
                .attribute("personId", CidlType::Integer, false, Some("Person".into()))
                .build(),
            ModelBuilder::new("Cat")
                .attribute("personId", CidlType::Integer, false, Some("Person".into()))
                .id()
                .build(),
            ModelBuilder::new("Person")
                .id()
                .nav_p(
                    "dogs",
                    CidlType::array(CidlType::Model("Dog".into())),
                    false,
                    CidlForeignKeyKind::OneToMany {
                        reference: "personId".into(),
                    },
                )
                .nav_p(
                    "cats",
                    CidlType::array(CidlType::Model("Cat".into())),
                    false,
                    CidlForeignKeyKind::OneToMany {
                        reference: "personId".into(),
                    },
                )
                .attribute("bossId", CidlType::Integer, false, Some("Boss".into()))
                .data_source(
                    "default",
                    IncludeTreeBuilder::default()
                        .add("dogs", CidlType::array(CidlType::Model("Dog".into())))
                        .add("cats", CidlType::array(CidlType::Model("Cat".into())))
                        .build(),
                )
                .build(),
            ModelBuilder::new("Boss")
                .id()
                .nav_p(
                    "persons",
                    CidlType::array(CidlType::Model("Person".into())),
                    false,
                    CidlForeignKeyKind::OneToMany {
                        reference: "bossId".into(),
                    },
                )
                .data_source(
                    "default",
                    IncludeTreeBuilder::default()
                        .add_with_children(
                            "persons",
                            CidlType::array(CidlType::Model("Person".into())),
                            |b| {
                                b.add("dogs", CidlType::array(CidlType::Model("Dog".into())))
                                    .add("cats", CidlType::array(CidlType::Model("Cat".into())))
                            },
                        )
                        .build(),
                )
                .build(),
        ]);

        let d1gen = D1Generator::new(cidl, create_wrangler());

        // Act
        let sql = d1gen.sql().expect("gen_sqlite to work");

        // Assert
        expected_str!(
            sql,
            r#"CREATE VIEW "Person_default" AS SELECT "Person"."id" AS "Person_id", "Person"."bossId" AS "Person_bossId", "Dog"."id" AS "Dog_id", "Dog"."personId" AS "Dog_personId", "Cat"."personId" AS "Cat_personId", "Cat"."id" AS "Cat_id" FROM "Person" LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId" LEFT JOIN "Cat" ON "Person"."id" = "Cat"."personId";"#
        );

        expected_str!(
            sql,
            r#"CREATE VIEW "Boss_default" AS SELECT "Boss"."id" AS "Boss_id", "Person"."id" AS "Person_id", "Person"."bossId" AS "Person_bossId", "Dog"."id" AS "Dog_id", "Dog"."personId" AS "Dog_personId", "Cat"."personId" AS "Cat_personId", "Cat"."id" AS "Cat_id" FROM "Boss" LEFT JOIN "Person" ON "Boss"."id" = "Person"."bossId" LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId" LEFT JOIN "Cat" ON "Person"."id" = "Cat"."personId";"#
        );
    }

    // Many to Many
    {
        // Arrange
        let cidl = create_cidl(vec![
            ModelBuilder::new("Student")
                .id()
                .nav_p(
                    "courses",
                    CidlType::array(CidlType::Model("Course".to_string())),
                    false,
                    CidlForeignKeyKind::ManyToMany {
                        unique_id: "StudentsCourses".into(),
                    },
                )
                .data_source(
                    "default",
                    IncludeTreeBuilder::default()
                        .add("courses", CidlType::array(CidlType::Model("Course".into())))
                        .build(),
                )
                .build(),
            ModelBuilder::new("Course")
                .id()
                .nav_p(
                    "students",
                    CidlType::array(CidlType::Model("Student".to_string())),
                    false,
                    CidlForeignKeyKind::ManyToMany {
                        unique_id: "StudentsCourses".into(),
                    },
                )
                .data_source(
                    "default",
                    IncludeTreeBuilder::default()
                        .add(
                            "students",
                            CidlType::array(CidlType::Model("Student".into())),
                        )
                        .build(),
                )
                .build(),
        ]);
        let d1gen = D1Generator::new(cidl, create_wrangler());

        // Act
        let sql = d1gen.sql().expect("gen_sqlite to work");
        expected_str!(sql, r#"CREATE TABLE "StudentsCourses""#);

        // Assert: Many-to-many view for Student
        expected_str!(
            sql,
            r#"CREATE VIEW "Student_default" AS SELECT "Student"."id" AS "Student_id", "Course"."id" AS "Course_id" FROM "Student" LEFT JOIN "StudentsCourses" ON "Student"."id" = "StudentsCourses"."Student_id" LEFT JOIN "Course" ON "StudentsCourses"."Course_id" = "Course"."id";"#
        );

        // Assert: Many-to-many view for Course
        expected_str!(
            sql,
            r#"CREATE VIEW "Course_default" AS SELECT "Course"."id" AS "Course_id", "Student"."id" AS "Student_id" FROM "Course" LEFT JOIN "StudentsCourses" ON "Course"."id" = "StudentsCourses"."Course_id" LEFT JOIN "Student" ON "StudentsCourses"."Student_id" = "Student"."id";"#
        );
    }
}

#[test]
fn test_duplicate_column_error() {
    // Arrange
    let cidl = create_cidl(vec![
        ModelBuilder::new("Person")
            .id()
            .attribute("name", CidlType::Integer, false, None)
            .attribute("name", CidlType::Real, true, None)
            .build(),
    ]);

    let d1gen = D1Generator::new(cidl, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(err, "Duplicate column names");
}

#[test]
fn test_duplicate_primary_key_error() {
    // Arrange
    let cidl = create_cidl(vec![
        ModelBuilder::new("Person")
            .pk("id1", CidlType::Integer)
            .pk("id2", CidlType::Integer)
            .build(),
    ]);

    let d1gen = D1Generator::new(cidl, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(err, "Duplicate primary keys");
}

#[test]
fn test_nullable_primary_key_error() {
    // Arrange
    let mut model = ModelBuilder::new("Person").id().build();
    model.attributes[0].value.nullable = true;

    let cidl = create_cidl(vec![model]);
    let d1gen = D1Generator::new(cidl, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(err, "A primary key cannot be nullable.");
}

#[test]
fn test_missing_primary_key_error() {
    // Arrange
    let cidl = create_cidl(vec![ModelBuilder::new("Person").build()]);

    let d1gen = D1Generator::new(cidl, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(err, "Missing primary key on model");
}

#[test]
fn test_duplicate_model_error() {
    // Arrange
    let cidl = create_cidl(vec![
        ModelBuilder::new("Person").id().build(),
        ModelBuilder::new("Person").id().build(),
    ]);

    // Act
    let d1gen = D1Generator::new(cidl, create_wrangler());
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(err, "Duplicate model name");
}

#[test]
fn test_unknown_foreign_key_error() {
    // Arrange
    let cidl = create_cidl(vec![
        ModelBuilder::new("User")
            .id()
            .attribute(
                "nonExistentId",
                CidlType::Integer,
                false,
                Some("NonExistent".to_string()),
            )
            .build(),
    ]);

    // Act
    let d1gen = D1Generator::new(cidl, create_wrangler());
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(
        err,
        "Unknown Model for foreign key User.nonExistentId => NonExistent?"
    );
}

#[test]
fn test_cycle_detection_error() {
    // Arrange
    // A -> B -> C -> A
    let cidl = create_cidl(vec![
        ModelBuilder::new("A")
            .id()
            .attribute("bId", CidlType::Integer, false, Some("B".to_string()))
            .build(),
        ModelBuilder::new("B")
            .id()
            .attribute("cId", CidlType::Integer, false, Some("C".to_string()))
            .build(),
        ModelBuilder::new("C")
            .id()
            .attribute("aId", CidlType::Integer, false, Some("A".to_string()))
            .build(),
    ]);

    // Act
    let d1gen = D1Generator::new(cidl, create_wrangler());
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(err, "Cycle detected");
}

#[test]
fn test_nullability_prevents_cycle_error() {
    // Arrange
    // A -> B -> C -> Nullable<A>
    let cidl = create_cidl(vec![
        ModelBuilder::new("A")
            .id()
            .attribute("bId", CidlType::Integer, false, Some("B".to_string()))
            .build(),
        ModelBuilder::new("B")
            .id()
            .attribute("cId", CidlType::Integer, false, Some("C".to_string()))
            .build(),
        ModelBuilder::new("C")
            .id()
            .attribute("aId", CidlType::Integer, true, Some("A".to_string()))
            .build(),
    ]);

    // Act
    let d1gen = D1Generator::new(cidl, create_wrangler());

    // Assert
    d1gen.sql().expect("sqlite gen to work");
}

#[test]
fn test_invalid_sqlite_type_error() {
    // Arrange
    let cidl = create_cidl(vec![
        ModelBuilder::new("BadType")
            .id()
            .attribute("attr", CidlType::Model("User".into()), false, None)
            .build(),
    ]);

    let d1gen = D1Generator::new(cidl, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(err, "Invalid SQL Type");
}

#[test]
fn test_one_to_one_nav_property_unknown_attribute_reference_error() {
    // Arrange
    let spec = create_cidl(vec![
        ModelBuilder::new("Dog").id().build(),
        ModelBuilder::new("Person")
            .id()
            .nav_p(
                "dog",
                CidlType::Model("Dog".into()),
                false,
                CidlForeignKeyKind::OneToOne {
                    reference: "dogId".to_string(),
                },
            )
            .build(),
    ]);

    let d1gen = D1Generator::new(spec, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(
        err,
        "Navigation property Person.dog references Dog.dogId which does not exist."
    );
}

#[test]
fn test_primary_key_cannot_be_foreign_key() {
    // Arrange: create an attribute that is both primary key and a foreign key
    let mut model = ModelBuilder::new("Person")
        .attribute("id", CidlType::Integer, false, Some("Other".into()))
        .build();
    model.attributes[0].primary_key = true;

    let cidl = create_cidl(vec![model]);
    let d1gen = D1Generator::new(cidl, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(err, "A primary key cannot be a foreign key");
}

#[test]
fn test_one_to_one_nav_property_expected_model_type_error() {
    // Arrange: nav prop has a non-model type (should be Model)
    let spec = create_cidl(vec![
        ModelBuilder::new("Dog").id().build(),
        ModelBuilder::new("Person")
            .id()
            // intentionally wrong nav type (integer)
            .nav_p(
                "dog",
                CidlType::Integer,
                false,
                CidlForeignKeyKind::OneToOne {
                    reference: "dogId".to_string(),
                },
            )
            .build(),
    ]);

    let d1gen = D1Generator::new(spec, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(
        err,
        "Expected Model type for navigation property Person.dog"
    );
}

#[test]
fn test_one_to_one_mismatched_fk_and_nav_type_error() {
    // Arrange: attribute dogId references Dog, but nav prop type is Cat -> mismatch
    let spec = create_cidl(vec![
        ModelBuilder::new("Dog").id().build(),
        ModelBuilder::new("Cat").id().build(),
        ModelBuilder::new("Person")
            .id()
            .attribute("dogId", CidlType::Integer, false, Some("Dog".into()))
            .nav_p(
                "dog",
                CidlType::Model("Cat".into()), // incorrect: says Cat but fk points to Dog
                false,
                CidlForeignKeyKind::OneToOne {
                    reference: "dogId".to_string(),
                },
            )
            .build(),
    ]);

    let d1gen = D1Generator::new(spec, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert - message includes "Mismatched types between foreign key and One to One navigation property"
    expected_str!(
        err,
        "Mismatched types between foreign key and One to One navigation property"
    );
}

#[test]
fn test_one_to_many_expected_collection_type_error() {
    // Arrange: one-to-many nav prop should be a collection (array), but is a single Model
    let spec = create_cidl(vec![
        ModelBuilder::new("Dog")
            .id()
            .attribute("personId", CidlType::Integer, false, Some("Person".into()))
            .build(),
        ModelBuilder::new("Person")
            .id()
            .nav_p(
                "dogs",
                CidlType::Model("Dog".into()), // wrong: not an array
                false,
                CidlForeignKeyKind::OneToMany {
                    reference: "personId".into(),
                },
            )
            .build(),
    ]);

    let d1gen = D1Generator::new(spec, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(
        err,
        "Expected collection of Model type for navigation property Person.dogs"
    );
}

#[test]
fn test_one_to_many_nullable_nav_property_error() {
    // Arrange: one-to-many nav property marked nullable (not allowed)
    let spec = create_cidl(vec![
        ModelBuilder::new("Dog").id().build(),
        ModelBuilder::new("Person")
            .id()
            .nav_p(
                "dogs",
                CidlType::Array(Box::new(CidlType::Model("Dog".into()))),
                true, // nullable -> should error
                CidlForeignKeyKind::OneToMany {
                    reference: "personId".into(),
                },
            )
            .build(),
    ]);

    let d1gen = D1Generator::new(spec, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(err, "Navigation property cannot be nullable Person.dogs");
}

#[test]
fn test_one_to_many_unknown_nav_model_error() {
    // Arrange: nav prop pointing to a non-existent model name
    let spec = create_cidl(vec![
        ModelBuilder::new("Person")
            .id()
            .nav_p(
                "dogs",
                CidlType::Array(Box::new(CidlType::Model("Dog".into()))),
                false,
                CidlForeignKeyKind::OneToMany {
                    reference: "personId".into(),
                },
            )
            .build(),
        // Note: Dog is missing
    ]);

    let d1gen = D1Generator::new(spec, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(
        err,
        "Unknown Model for navigation property Person.dogs => Dog?"
    );
}

#[test]
fn test_one_to_many_unresolved_reference_error() {
    // Arrange:
    // Person declares OneToMany to Dog referencing Dog.personId, but Dog has no personId FK attr.
    let spec = create_cidl(vec![
        ModelBuilder::new("Dog").id().build(), // no personId attribute
        ModelBuilder::new("Person")
            .id()
            .nav_p(
                "dogs",
                CidlType::Array(Box::new(CidlType::Model("Dog".to_string()))),
                false,
                CidlForeignKeyKind::OneToMany {
                    reference: "personId".into(),
                },
            )
            .build(),
    ]);

    let d1gen = D1Generator::new(spec, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(
        err,
        "Navigation property Person.dogs references Dog.personId which does not exist."
    );
}

#[test]
fn test_many_to_many_expected_collection_type_error() {
    // Arrange: many-to-many nav prop should be an array of Model, not a single Model
    let cidl = create_cidl(vec![
        ModelBuilder::new("Student")
            .id()
            .nav_p(
                "courses",
                CidlType::Model("Course".into()), // wrong: not array
                false,
                CidlForeignKeyKind::ManyToMany {
                    unique_id: "StudentsCourses".into(),
                },
            )
            .build(),
        ModelBuilder::new("Course").id().build(),
    ]);

    let d1gen = D1Generator::new(cidl, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(
        err,
        "Expected collection of Model type for navigation property Student.courses"
    );
}

#[test]
fn test_many_to_many_nullable_nav_property_error() {
    // Arrange: many-to-many nav property marked nullable (not allowed)
    let cidl = create_cidl(vec![
        ModelBuilder::new("Student")
            .id()
            .nav_p(
                "courses",
                CidlType::Array(Box::new(CidlType::Model("Course".into()))),
                true, // nullable -> should error
                CidlForeignKeyKind::ManyToMany {
                    unique_id: "StudentsCourses".into(),
                },
            )
            .build(),
        ModelBuilder::new("Course").id().build(),
    ]);

    let d1gen = D1Generator::new(cidl, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(
        err,
        "Navigation property cannot be nullable Student.courses"
    );
}

#[test]
fn test_many_to_many_unknown_nav_model_error() {
    // Arrange: ManyToMany nav pointing at a non-existent model
    let cidl = create_cidl(vec![
        ModelBuilder::new("Student")
            .id()
            .nav_p(
                "courses",
                CidlType::Array(Box::new(CidlType::Model("Course".into()))),
                false,
                CidlForeignKeyKind::ManyToMany {
                    unique_id: "StudentsCourses".into(),
                },
            )
            .build(),
        // Course missing
    ]);

    let d1gen = D1Generator::new(cidl, create_wrangler());

    // Act
    let err = d1gen.sql().unwrap_err();

    // Assert
    expected_str!(
        err,
        "Unknown Model for navigation property Student.courses => Course?"
    );
}

#[test]
fn test_junction_table_builder_errors() {
    // Missing second nav property case: only one side of many-to-many
    {
        let cidl = create_cidl(vec![
            ModelBuilder::new("Student")
                .id()
                .nav_p(
                    "courses",
                    CidlType::Array(Box::new(CidlType::Model("Course".into()))),
                    false,
                    CidlForeignKeyKind::ManyToMany {
                        unique_id: "OnlyOne".into(),
                    },
                )
                .build(),
            // Course exists, but doesn't declare the reciprocal nav property
            ModelBuilder::new("Course").id().build(),
        ]);

        let d1gen = D1Generator::new(cidl, create_wrangler());
        let err = d1gen.sql().unwrap_err();
        expected_str!(err, "Both models must be set for a junction table");
    }

    // Too many models case: three models register the same junction id
    {
        let cidl = create_cidl(vec![
            ModelBuilder::new("A")
                .id()
                .nav_p(
                    "bs",
                    CidlType::Array(Box::new(CidlType::Model("B".into()))),
                    false,
                    CidlForeignKeyKind::ManyToMany {
                        unique_id: "TriJ".into(),
                    },
                )
                .build(),
            ModelBuilder::new("B")
                .id()
                .nav_p(
                    "as",
                    CidlType::Array(Box::new(CidlType::Model("A".into()))),
                    false,
                    CidlForeignKeyKind::ManyToMany {
                        unique_id: "TriJ".into(),
                    },
                )
                .build(),
            // Third model C tries to use the same junction id -> should error
            ModelBuilder::new("C")
                .id()
                .nav_p(
                    "as",
                    CidlType::Array(Box::new(CidlType::Model("A".into()))),
                    false,
                    CidlForeignKeyKind::ManyToMany {
                        unique_id: "TriJ".into(),
                    },
                )
                .build(),
        ]);

        let d1gen = D1Generator::new(cidl, create_wrangler());
        let err = d1gen.sql().unwrap_err();
        expected_str!(
            err,
            "Too many ManyToMany navigation properties for junction table"
        );
    }
}
