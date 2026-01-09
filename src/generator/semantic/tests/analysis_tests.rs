use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use ast::{
    CidlType, HttpVerb, MigrationsAst, NamedTypedValue, NavigationPropertyKind, Service,
    ServiceAttribute, WranglerEnv, err::GeneratorErrorKind,
};
use generator_test::{ModelBuilder, create_ast, create_spec};
use semantic::SemanticAnalysis;
use wrangler::WranglerGenerator;

#[test]
fn cloesce_serializes_to_migrations() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Dog").id_pk().build(),
        ModelBuilder::new("Person").id_pk().build(),
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
fn null_primary_key_error() {
    // Arrange
    let mut model = ModelBuilder::new("Dog").build();
    model.primary_key = Some(NamedTypedValue {
        name: "id".into(),
        cidl_type: CidlType::nullable(CidlType::Integer),
    });

    let mut ast = create_ast(vec![model]);
    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

    // Assert
    assert!(matches!(err.kind, GeneratorErrorKind::NullPrimaryKey));
}

#[test]
fn mismatched_foreign_keys_error() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Person")
            .id_pk()
            .col("dogId", CidlType::Text, Some("Dog".into()))
            .build(),
        ModelBuilder::new("Dog").id_pk().build(),
    ]);
    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

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
            .id_pk()
            .col("bId", CidlType::Integer, Some("B".to_string()))
            .build(),
        ModelBuilder::new("B")
            .id_pk()
            .col("cId", CidlType::Integer, Some("C".to_string()))
            .build(),
        ModelBuilder::new("C")
            .id_pk()
            .col("aId", CidlType::Integer, Some("A".to_string()))
            .build(),
    ]);
    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

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
            attributes: vec![ServiceAttribute {
                var_name: "b".into(),
                inject_reference: "B".into(),
            }],
            methods: BTreeMap::default(),
            source_path: PathBuf::default(),
        },
        Service {
            name: "B".into(),
            attributes: vec![ServiceAttribute {
                var_name: "c".into(),
                inject_reference: "C".into(),
            }],
            methods: BTreeMap::default(),
            source_path: PathBuf::default(),
        },
        Service {
            name: "C".into(),
            attributes: vec![ServiceAttribute {
                var_name: "a".into(),
                inject_reference: "A".into(),
            }],
            methods: BTreeMap::default(),
            source_path: PathBuf::default(),
        },
    ];
    ast.services = services.into_iter().map(|s| (s.name.clone(), s)).collect();

    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

    // Assert
    assert!(matches!(err.kind, GeneratorErrorKind::CyclicalDependency));
    assert!(err.context.contains("A, B, C"));
}

#[test]
fn model_attr_nullability_prevents_cycle_error() {
    // Arrange
    // A -> B -> C -> Nullable<A>
    let mut ast = create_ast(vec![
        ModelBuilder::new("A")
            .id_pk()
            .col("bId", CidlType::Integer, Some("B".to_string()))
            .build(),
        ModelBuilder::new("B")
            .id_pk()
            .col("cId", CidlType::Integer, Some("C".to_string()))
            .build(),
        ModelBuilder::new("C")
            .id_pk()
            .col(
                "aId",
                CidlType::nullable(CidlType::Integer),
                Some("A".to_string()),
            )
            .build(),
    ]);
    let spec = create_spec(&ast);

    // Act
    SemanticAnalysis::analyze(&mut ast, &spec).expect("analysis to pass");
}

#[test]
fn one_to_one_nav_property_unknown_attribute_reference_error() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Dog").id_pk().build(),
        ModelBuilder::new("Person")
            .id_pk()
            .nav_p(
                "dog",
                "Dog",
                NavigationPropertyKind::OneToOne {
                    column_reference: "dogId".to_string(),
                },
            )
            .build(),
    ]);
    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

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
        ModelBuilder::new("Dog").id_pk().build(),
        ModelBuilder::new("Cat").id_pk().build(),
        ModelBuilder::new("Person")
            .id_pk()
            .col("dogId", CidlType::Integer, Some("Dog".into()))
            .nav_p(
                "dog",
                "Cat", // incorrect: says Cat but fk points to Dog
                NavigationPropertyKind::OneToOne {
                    column_reference: "dogId".to_string(),
                },
            )
            .build(),
    ]);
    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

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
        ModelBuilder::new("Dog").id_pk().build(), // no personId attribute
        ModelBuilder::new("Person")
            .id_pk()
            .nav_p(
                "dogs",
                "Dog",
                NavigationPropertyKind::OneToMany {
                    column_reference: "personId".into(),
                },
            )
            .build(),
    ]);
    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

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
                .id_pk()
                .nav_p("courses", "Course", NavigationPropertyKind::ManyToMany)
                .build(),
            // Course exists, but doesn't declare the reciprocal nav property
            ModelBuilder::new("Course").id_pk().build(),
        ]);
        let spec = create_spec(&ast);

        let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();
        assert!(matches!(
            err.kind,
            GeneratorErrorKind::MissingManyToManyReference
        ));
    }

    // Too many models case: three models register the same junction id
    {
        let mut ast = create_ast(vec![
            ModelBuilder::new("A")
                .id_pk()
                .nav_p("bs", "B", NavigationPropertyKind::ManyToMany)
                .build(),
            ModelBuilder::new("B")
                .id_pk()
                .nav_p("as", "A", NavigationPropertyKind::ManyToMany)
                .build(),
            // Third model C tries to use the same junction id -> should error
            ModelBuilder::new("C")
                .id_pk()
                .nav_p("as", "A", NavigationPropertyKind::ManyToMany)
                .build(),
        ]);
        let spec = create_spec(&ast);

        let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();
        assert!(matches!(
            err.kind,
            GeneratorErrorKind::ExtraneousManyToManyReferences
        ));
    }
}

#[test]
fn instantiated_stream_method() {
    // Arrange
    let model = ModelBuilder::new("Dog")
        .id_pk()
        .method(
            "uploadPhoto",
            HttpVerb::POST,
            false,
            vec![
                NamedTypedValue {
                    name: "stream".into(),
                    cidl_type: CidlType::Stream,
                },
                NamedTypedValue {
                    name: "ds".into(),
                    cidl_type: CidlType::DataSource("Dog".into()),
                },
            ],
            CidlType::Stream,
        )
        .build();

    let mut ast = create_ast(vec![model]);
    let spec = create_spec(&ast);

    // Act
    let res = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    res.unwrap();
}

#[test]
fn static_stream_method() {
    // Arrange
    let model = ModelBuilder::new("Dog")
        .id_pk()
        .method(
            "uploadPhoto",
            HttpVerb::POST,
            true,
            vec![NamedTypedValue {
                name: "stream".into(),
                cidl_type: CidlType::Stream,
            }],
            CidlType::Stream,
        )
        .build();

    let mut ast = create_ast(vec![model]);
    let spec = create_spec(&ast);

    // Act
    let res = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    res.unwrap();
}

#[test]
fn invalid_stream_method() {
    // Arrange
    let model = ModelBuilder::new("Dog")
        .id_pk()
        .method(
            "uploadPhoto",
            HttpVerb::POST,
            true, // static is true, can only have 1 param
            vec![
                NamedTypedValue {
                    name: "stream".into(),
                    cidl_type: CidlType::Stream,
                },
                NamedTypedValue {
                    name: "ds".into(),
                    cidl_type: CidlType::DataSource("Dog".into()),
                },
            ],
            CidlType::Stream,
        )
        .build();

    let mut ast = create_ast(vec![model]);
    let spec = create_spec(&ast);

    // Act
    let res = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    assert!(matches!(
        res.unwrap_err().kind,
        GeneratorErrorKind::InvalidStream
    ));
}

#[test]
fn missing_variable_in_wrangler() {
    // Arrange
    let mut ast = create_ast(vec![ModelBuilder::new("User").id_pk().build()]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: None,
        kv_bindings: vec![],
        r2_bindings: vec![],
        vars: [
            ("API_KEY".into(), ast::CidlType::Text),
            ("TIMEOUT".into(), ast::CidlType::Integer),
        ]
        .into_iter()
        .collect(),
    });

    let specs = vec![
        WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
        WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
    ];

    for spec in specs {
        // Act
        let res = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind;

        // Assert
        assert!(matches!(res, GeneratorErrorKind::MissingWranglerVariable));
    }
}

#[test]
fn missing_env_in_ast() {
    // Arrange
    let mut ast = create_ast(vec![ModelBuilder::new("User").id_pk().build()]);
    ast.wrangler_env = None;

    let specs = vec![
        WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
        WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
    ];

    for spec in specs {
        // Act
        let res = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind;

        // Assert
        assert!(matches!(res, GeneratorErrorKind::MissingWranglerEnv));
    }
}

#[test]
fn missing_d1_binding_in_wrangler() {
    // Arrange
    let mut ast = create_ast(vec![ModelBuilder::new("User").id_pk().build()]);

    let specs = vec![
        WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
        WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
    ];

    for spec in specs {
        // Act
        let res = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind;

        // Assert
        assert!(matches!(res, GeneratorErrorKind::MissingWranglerD1Binding));
    }
}

#[test]
fn missing_kv_bindings_in_wrangler() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Settings")
            .kv_object(
                "settings",
                "my_binding",
                "settings",
                false,
                CidlType::JsonValue,
            )
            .build(),
    ]);

    let specs = vec![
        WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
        WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
    ];

    for spec in specs {
        // Act
        let res = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind;

        // Assert
        assert!(matches!(
            res,
            GeneratorErrorKind::MissingWranglerKVNamespace
        ));
    }
}

#[test]
fn kv_object_valid_key_format() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Settings")
            .id_pk()
            .col("userId", CidlType::Integer, None)
            .key_param("tenant")
            .kv_object(
                "settings/{tenant}/{userId}",
                "my_kv",
                "config",
                false,
                CidlType::JsonValue,
            )
            .build(),
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: Some("my_d1".into()),
        kv_bindings: vec!["my_kv".into()],
        r2_bindings: vec![],
        vars: HashMap::new(),
    });

    let spec = create_spec(&ast);

    // Act
    let result = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    result.expect("analysis to pass");
}

#[test]
fn kv_object_missing_key_param_error() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Settings")
            .id_pk()
            .kv_object(
                "settings/{tenant}/{userId}",
                "my_kv",
                "config",
                false,
                CidlType::JsonValue,
            )
            .build(),
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: Some("my_d1".into()),
        kv_bindings: vec!["my_kv".into()],
        r2_bindings: vec![],
        vars: HashMap::new(),
    });
    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

    // Assert
    assert!(matches!(err.kind, GeneratorErrorKind::UnknownKeyReference));
}

#[test]
fn kv_object_with_primary_key_in_format() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("User")
            .id_pk()
            .kv_object(
                "user/{id}/preferences",
                "my_kv",
                "prefs",
                false,
                CidlType::JsonValue,
            )
            .build(),
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: Some("my_d1".into()),
        kv_bindings: vec!["my_kv".into()],
        r2_bindings: vec![],
        vars: HashMap::new(),
    });
    let spec = create_spec(&ast);

    // Act
    let result = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    result.expect("analysis to pass");
}

#[test]
fn kv_object_with_column_in_format() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Session")
            .id_pk()
            .col("token", CidlType::Text, None)
            .kv_object(
                "session/{token}",
                "my_kv",
                "data",
                false,
                CidlType::JsonValue,
            )
            .build(),
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: Some("my_d1".into()),
        kv_bindings: vec!["my_kv".into()],
        r2_bindings: vec![],
        vars: HashMap::new(),
    });
    let spec = create_spec(&ast);

    // Act
    let result = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    result.expect("analysis to pass");
}

#[test]
fn kv_object_invalid_nested_braces_error() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Settings")
            .id_pk()
            .kv_object(
                "settings/{{nested}}",
                "my_kv",
                "config",
                false,
                CidlType::JsonValue,
            )
            .build(),
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: Some("my_d1".into()),
        kv_bindings: vec!["my_kv".into()],
        r2_bindings: vec![],
        vars: HashMap::new(),
    });
    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

    // Assert
    assert!(matches!(err.kind, GeneratorErrorKind::InvalidKeyFormat));
}

#[test]
fn kv_object_unclosed_brace_error() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("Settings")
            .id_pk()
            .kv_object(
                "settings/{unclosed",
                "my_kv",
                "config",
                false,
                CidlType::JsonValue,
            )
            .build(),
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: Some("my_d1".into()),
        kv_bindings: vec!["my_kv".into()],
        r2_bindings: vec![],
        vars: HashMap::new(),
    });
    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

    // Assert
    assert!(matches!(err.kind, GeneratorErrorKind::InvalidKeyFormat));
}
