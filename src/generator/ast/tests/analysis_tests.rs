use std::collections::BTreeMap;

use ast::{
    CidlType, MigrationsAst, NavigationPropertyKind, Service,
    builder::{ModelBuilder, create_ast},
    err::GeneratorErrorKind,
};

#[test]
fn cloesce_serializes_to_migrations() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Dog").id().build(),
        ModelBuilder::new("Person").id().build(),
    ]);
    ast.set_merkle_hash();

    // Act
    let json = ast.to_migrations_json();
    let migrations_ast = serde_json::from_str::<MigrationsAst>(&json).expect("serde to pass");

    // Assert
    assert!(migrations_ast.hash != 0u64);
    assert!(migrations_ast.models.contains_key("Dog"));
    assert!(migrations_ast.models.contains_key("Person"));
    assert!(migrations_ast.models[0].hash != 0u64);
}

#[test]
fn null_primary_key() {
    // Arrange
    let mut model = ModelBuilder::new("Dog").id().build();
    model.primary_key.cidl_type = CidlType::nullable(CidlType::Integer);

    let mut ast = create_ast(vec![model]);

    // Act
    let err = ast.semantic_analysis().unwrap_err();

    // Assert
    assert!(matches!(err.kind, GeneratorErrorKind::NullPrimaryKey));
}

#[test]
fn mismatched_foreign_keys() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Person")
            .id()
            .attribute("dogId", CidlType::Text, Some("Dog".into()))
            .build(),
        ModelBuilder::new("Dog").id().build(),
    ]);

    // Act
    let err = ast.semantic_analysis().unwrap_err();

    // Assert
    assert!(matches!(
        err.kind,
        GeneratorErrorKind::MismatchedForeignKeyTypes
    ));
}

#[test]
fn model_cycle_detection_error() {
    // Arrange
    let mut ast = create_ast(vec![
        // A -> B -> C -> A
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
    let err = ast.semantic_analysis().unwrap_err();

    // Assert
    assert!(matches!(err.kind, GeneratorErrorKind::CyclicalDependency));
    assert!(err.context.contains("A, B, C"));
}

#[test]
fn service_cycle_detection_error() {
    // Arrange
    let mut ast = create_ast(vec![]);
    let services = vec![
        // A -> B -> C -> A
        Service {
            name: "A".into(),
            injected: vec!["B".into()],
            methods: BTreeMap::default(),
        },
        Service {
            name: "B".into(),
            injected: vec!["C".into()],
            methods: BTreeMap::default(),
        },
        Service {
            name: "C".into(),
            injected: vec!["A".into()],
            methods: BTreeMap::default(),
        },
    ];
    ast.services = services.into_iter().map(|s| (s.name.clone(), s)).collect();

    // Act
    let err = ast.semantic_analysis().unwrap_err();

    // Assert
    assert!(matches!(err.kind, GeneratorErrorKind::CyclicalDependency));
    assert!(err.context.contains("A, B, C"));
}

#[test]
fn nullability_prevents_cycle_error() {
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
    ast.semantic_analysis().expect("analysis to pass");
}

#[test]
fn one_to_one_nav_property_unknown_attribute_reference_error() {
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
    let err = ast.semantic_analysis().unwrap_err();

    // Assert
    assert!(matches!(
        err.kind,
        GeneratorErrorKind::InvalidNavigationPropertyReference
    ));
}

#[test]
fn one_to_one_mismatched_fk_and_nav_type_error() {
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
    let err = ast.semantic_analysis().unwrap_err();

    // Assert
    assert!(matches!(
        err.kind,
        GeneratorErrorKind::MismatchedNavigationPropertyTypes
    ));
}

#[test]
fn one_to_many_unresolved_reference_error() {
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
    let err = ast.semantic_analysis().unwrap_err();

    // Assert
    assert!(err.context.contains(
        "Person.dogs references Dog.personId which does not exist or is not a foreign key to Person"
    ));
}

#[test]
fn junction_table_builder_errors() {
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

        let err = ast.semantic_analysis().unwrap_err();
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

        let err = ast.semantic_analysis().unwrap_err();
        assert!(matches!(
            err.kind,
            GeneratorErrorKind::ExtraneousManyToManyReferences
        ));
    }
}
