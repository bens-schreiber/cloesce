use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
};

use ast::{
    CidlType, D1NavigationPropertyKind, HttpVerb, MigrationsAst, NamedTypedValue, Service,
    ServiceAttribute, WranglerEnv, err::GeneratorErrorKind,
};
use generator_test::{D1ModelBuilder, KVModelBuilder, create_ast_d1, create_ast_kv, create_spec};
use semantic::SemanticAnalysis;
use wrangler::WranglerGenerator;

#[test]
fn cloesce_serializes_to_migrations() {
    // Arrange
    let mut ast = create_ast_d1(vec![
        D1ModelBuilder::new("Dog").id().build(),
        D1ModelBuilder::new("Person").id().build(),
    ]);
    ast.set_merkle_hash();

    // Act
    let json = ast.to_migrations_json();
    let migrations_ast = serde_json::from_str::<MigrationsAst>(&json).expect("serde to pass");

    // Assert
    assert!(migrations_ast.hash != 0u64);
    assert!(migrations_ast.d1_models.contains_key("Dog"));
    assert!(migrations_ast.d1_models.contains_key("Person"));
    assert!(migrations_ast.d1_models[0].hash != 0u64);
}

#[test]
fn null_primary_key_error() {
    // Arrange
    let mut model = D1ModelBuilder::new("Dog").id().build();
    model.primary_key.cidl_type = CidlType::nullable(CidlType::Integer);

    let mut ast = create_ast_d1(vec![model]);
    let spec = create_spec(&ast);

    // Act
    let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

    // Assert
    assert!(matches!(err.kind, GeneratorErrorKind::NullPrimaryKey));
}

#[test]
fn mismatched_foreign_keys_error() {
    // Arrange
    let mut ast = create_ast_d1(vec![
        D1ModelBuilder::new("Person")
            .id()
            .attribute("dogId", CidlType::Text, Some("Dog".into()))
            .build(),
        D1ModelBuilder::new("Dog").id().build(),
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
    let mut ast = create_ast_d1(vec![
        // A -> B -> C -> A
        D1ModelBuilder::new("A")
            .id()
            .attribute("bId", CidlType::Integer, Some("B".to_string()))
            .build(),
        D1ModelBuilder::new("B")
            .id()
            .attribute("cId", CidlType::Integer, Some("C".to_string()))
            .build(),
        D1ModelBuilder::new("C")
            .id()
            .attribute("aId", CidlType::Integer, Some("A".to_string()))
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
    let mut ast = create_ast_d1(vec![]);
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
    let mut ast = create_ast_d1(vec![
        D1ModelBuilder::new("A")
            .id()
            .attribute("bId", CidlType::Integer, Some("B".to_string()))
            .build(),
        D1ModelBuilder::new("B")
            .id()
            .attribute("cId", CidlType::Integer, Some("C".to_string()))
            .build(),
        D1ModelBuilder::new("C")
            .id()
            .attribute(
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
    let mut ast = create_ast_d1(vec![
        D1ModelBuilder::new("Dog").id().build(),
        D1ModelBuilder::new("Person")
            .id()
            .nav_p(
                "dog",
                "Dog",
                D1NavigationPropertyKind::OneToOne {
                    attribute_reference: "dogId".to_string(),
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
    let mut ast = create_ast_d1(vec![
        D1ModelBuilder::new("Dog").id().build(),
        D1ModelBuilder::new("Cat").id().build(),
        D1ModelBuilder::new("Person")
            .id()
            .attribute("dogId", CidlType::Integer, Some("Dog".into()))
            .nav_p(
                "dog",
                "Cat", // incorrect: says Cat but fk points to Dog
                D1NavigationPropertyKind::OneToOne {
                    attribute_reference: "dogId".to_string(),
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
    let mut ast = create_ast_d1(vec![
        D1ModelBuilder::new("Dog").id().build(), // no personId attribute
        D1ModelBuilder::new("Person")
            .id()
            .nav_p(
                "dogs",
                "Dog",
                D1NavigationPropertyKind::OneToMany {
                    attribute_reference: "personId".into(),
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
        let mut ast = create_ast_d1(vec![
            D1ModelBuilder::new("Student")
                .id()
                .nav_p(
                    "courses",
                    "Course",
                    D1NavigationPropertyKind::ManyToMany {
                        unique_id: "OnlyOne".into(),
                    },
                )
                .build(),
            // Course exists, but doesn't declare the reciprocal nav property
            D1ModelBuilder::new("Course").id().build(),
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
        let mut ast = create_ast_d1(vec![
            D1ModelBuilder::new("A")
                .id()
                .nav_p(
                    "bs",
                    "B",
                    D1NavigationPropertyKind::ManyToMany {
                        unique_id: "TriJ".into(),
                    },
                )
                .build(),
            D1ModelBuilder::new("B")
                .id()
                .nav_p(
                    "as",
                    "A",
                    D1NavigationPropertyKind::ManyToMany {
                        unique_id: "TriJ".into(),
                    },
                )
                .build(),
            // Third model C tries to use the same junction id -> should error
            D1ModelBuilder::new("C")
                .id()
                .nav_p(
                    "as",
                    "A",
                    D1NavigationPropertyKind::ManyToMany {
                        unique_id: "TriJ".into(),
                    },
                )
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
    let model = D1ModelBuilder::new("Dog")
        .id()
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

    let mut ast = create_ast_d1(vec![model]);
    let spec = create_spec(&ast);

    // Act
    let res = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    res.unwrap();
}

#[test]
fn static_stream_method() {
    // Arrange
    let model = D1ModelBuilder::new("Dog")
        .id()
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

    let mut ast = create_ast_d1(vec![model]);
    let spec = create_spec(&ast);

    // Act
    let res = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    res.unwrap();
}

#[test]
fn invalid_stream_method() {
    // Arrange
    let model = D1ModelBuilder::new("Dog")
        .id()
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

    let mut ast = create_ast_d1(vec![model]);
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
    let mut ast = create_ast_d1(vec![D1ModelBuilder::new("User").id().build()]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: None,
        kv_bindings: vec![],
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

    // Act + Assert
    for spec in specs {
        assert!(matches!(
            SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind,
            GeneratorErrorKind::MissingWranglerVariable
        ));
    }
}

#[test]
fn missing_env_in_ast() {
    // Arrange
    let mut ast = create_ast_d1(vec![D1ModelBuilder::new("User").id().build()]);
    ast.wrangler_env = None;

    let specs = vec![
        WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
        WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
    ];

    // Act + Assert
    for spec in specs {
        assert!(matches!(
            SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind,
            GeneratorErrorKind::MissingWranglerEnv
        ));
    }
}

#[test]
fn missing_d1_binding_in_wrangler() {
    // Arrange
    let mut ast = create_ast_d1(vec![D1ModelBuilder::new("User").id().build()]);

    let specs = vec![
        WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
        WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
    ];

    // Act + Assert
    for spec in specs {
        assert!(matches!(
            SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind,
            GeneratorErrorKind::MissingWranglerD1Binding
        ));
    }
}

#[test]
fn missing_kv_bindings_in_wrangler() {
    // Arrange
    let mut ast = create_ast_kv(vec![
        KVModelBuilder::new("MyKV", "MY_KV_BINDING", CidlType::JsonValue).build(),
    ]);

    let specs = vec![
        WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
        WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
    ];

    // Act + Assert
    for spec in specs {
        assert!(matches!(
            SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind,
            GeneratorErrorKind::MissingWranglerKVNamespace
        ));
    }
}

#[test]
fn detect_kv_graph() {
    // Arrange
    let mut ast = create_ast_kv(vec![
        // A -> B; C -> B
        KVModelBuilder::new("A", "KV1", CidlType::JsonValue)
            .model_nav_p("B", "b", false)
            .build(),
        KVModelBuilder::new("B", "KV1", CidlType::JsonValue).build(),
        KVModelBuilder::new("C", "KV1", CidlType::JsonValue)
            .model_nav_p("B", "b", false)
            .build(),
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: None,
        kv_bindings: [("KV1".into())].into_iter().collect(),
        vars: HashMap::default(),
    });

    let spec = create_spec(&ast);

    // Act
    let res = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    assert!(matches!(
        res.unwrap_err().kind,
        GeneratorErrorKind::InvalidKVTree
    ));
}

#[test]
fn mismatched_kv_namespaces() {
    // Arrange
    let mut ast = create_ast_kv(vec![
        KVModelBuilder::new("A", "KV1", CidlType::JsonValue)
            .model_nav_p("B", "b", false)
            .build(),
        KVModelBuilder::new("B", "KV2", CidlType::JsonValue).build(), // different namespace
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: None,
        kv_bindings: [("KV1".into()), ("KV2".into())].into_iter().collect(),
        vars: HashMap::default(),
    });

    let spec = create_spec(&ast);

    // Act
    let res = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    assert!(matches!(
        res.unwrap_err().kind,
        GeneratorErrorKind::MismatchedKVModelNamespaces
    ));
}

#[test]
fn kv_attribute_model_many_missing_param() {
    // Arrange
    let mut ast = create_ast_kv(vec![
        KVModelBuilder::new("A", "KV1", CidlType::JsonValue)
            .model_nav_p("B", "b", true) // many=true, but B not defined with array type
            .build(),
        KVModelBuilder::new("B", "KV1", CidlType::JsonValue).build(),
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: None,
        kv_bindings: [("KV1".into())].into_iter().collect(),
        vars: HashMap::default(),
    });

    let spec = create_spec(&ast);

    // Act
    let res = SemanticAnalysis::analyze(&mut ast, &spec);

    // Assert
    assert!(matches!(
        res.unwrap_err().kind,
        GeneratorErrorKind::InvalidKVTree
    ));
}
