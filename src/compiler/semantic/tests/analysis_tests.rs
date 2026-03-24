use std::collections::HashMap;

use ast::{D1Database, KVNamespace, R2Bucket, WranglerSpec};
use compiler_test::lex_and_parse;
use semantic::{SemanticAnalysis, err::CompilerErrorKind};

// TODO: use wrangler defaults
fn create_spec() -> WranglerSpec {
    WranglerSpec {
        d1_databases: vec![D1Database {
            binding: Some("my_d1".into()),
            database_name: None,
            database_id: None,
            migrations_dir: None,
        }],
        kv_namespaces: vec![KVNamespace {
            binding: Some("my_kv".into()),
            id: None,
        }],
        r2_buckets: vec![R2Bucket {
            binding: Some("my_r2".into()),
            bucket_name: None,
        }],
        vars: HashMap::new(),
        name: None,
        compatibility_date: None,
        main: None,
    }
}

fn with_env(src: &str) -> String {
    format!(
        r#"
        env {{
            my_d1: d1
            my_kv: kv 
            my_r2: r2
        }}

        {}
    "#,
        src
    )
}

fn assert_errors_eq(got: Vec<CompilerErrorKind>, expected: Vec<CompilerErrorKind>) {
    let mut got_sorted = got.clone();
    got_sorted.sort();
    let mut expected_sorted = expected.clone();
    expected_sorted.sort();
    assert_eq!(got_sorted, expected_sorted);
}

#[test]
fn multiple_wrangler_env_blocks() {
    // Arrange
    let src = r#"
        env {}
        env {}
    "#;
    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(
        res.unwrap_err(),
        vec![CompilerErrorKind::MultipleWranglerEnvBlocks]
    );
}

#[test]
fn missing_wrangler_env_block() {
    // Arrange
    let src = r#"
        model User {}
    "#;
    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(
        res.unwrap_err(),
        vec![CompilerErrorKind::MissingWranglerEnvBlock]
    );
}

#[test]
fn wrangler_binding_inconsistent_with_spec() {
    // Arrange
    let src = r#"
        env {
            my_d1: d1
            my_kv: kv 
            my_r2: r2
            other_d1: d1 // NOT consistent with the spec
        }
    "#;
    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(
        res.unwrap_err(),
        vec![CompilerErrorKind::WranglerBindingInconsistentWithSpec]
    );
}

#[test]
fn wrangler_duplicate_symbol() {
    // Arrange
    let src = r#"
        env {
            my_d1: d1
            my_kv: kv 
            my_r2: r2
            my_d1: d1 // duplicate symbol
        }
    "#;
    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_eq!(res.unwrap_err(), vec![CompilerErrorKind::DuplicateSymbol]);
}

#[test]
fn d1_model_basic_errors() {
    // Arrange
    let src = with_env(
        r#"
        @d1(my_d1)
        model User {
            // missing primary key
        }

        @d1(other_d1) // unresolved, not in spec
        model Post {}

        // missing binding
        model Comment {
            [primary id]
            id: Integer
        }
    "#,
    );
    let parse = lex_and_parse(&src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_errors_eq(
        res.unwrap_err(),
        vec![
            CompilerErrorKind::UnresolvedSymbol,
            CompilerErrorKind::D1ModelMissingPrimaryKey,
            CompilerErrorKind::D1ModelMissingD1Binding,
        ],
    );
}

#[test]
fn d1_model_column_fk_errors() {
    // Arrange
    let src = r#"
        env {
            my_d1: d1
            my_kv: kv
            my_r2: r2
            other_d1: d1
        }

        @d1(my_d1)
        model User {
            [primary id]
            id: Option<int> // primary key cannot be nullable
            id: int // duplicate symbol
            name: Object // invalid column type
            value: int
            str_value: string

            [foreign value -> Post::invalid] // invalid foreign key reference
            [foreign str_value -> Post::id] // foreign key references incompatible column type
            [foreign value -> User::id] // foreign key cannot reference same model
            [foreign value -> NonD1Model::id] // foreign key references non-d1 model
            [foreign value -> OtherD1Model::id] // foreign key references model in different database
        }

        @d1(my_d1)
        model Post {
            [primary id]
            id: int
        }

        @d1(other_d1)
        model OtherD1Model {
            [primary id]
            id: int
        }

        model NonD1Model { }
    "#;
    let mut spec = create_spec();
    spec.d1_databases.push(D1Database {
        binding: Some("other_d1".into()),
        database_name: None,
        database_id: None,
        migrations_dir: None,
    });

    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &spec);

    // Assert
    assert_errors_eq(
        res.unwrap_err(),
        vec![
            CompilerErrorKind::NullablePrimaryKey,
            CompilerErrorKind::DuplicateSymbol,
            CompilerErrorKind::InvalidColumnType,
            CompilerErrorKind::ForeignKeyReferencesInvalidOrUnknownColumn,
            CompilerErrorKind::ForeignKeyReferencesIncompatibleColumnType,
            CompilerErrorKind::ForeignKeyReferenceSelf,
            CompilerErrorKind::ForeignKeyReferencesNonD1Model,
            CompilerErrorKind::ForeignKeyReferencesDifferentDatabase,
        ],
    );
}

#[test]
fn d1_model_consistent_nullability_error() {
    // Arrange
    let src = r#"
        env {
            my_d1: d1
        }

        @d1(my_d1)
        model User {
            [primary id, name]
            id: int

            postId: Option<int>
            name: string

            [foreign (postId, name) -> (Post::id, Post::title)] // inconsistent nullability
        }

        @d1(my_d1)
        model Post {
            [primary id, title]
            id: int
            title: string
        }
    "#;

    let parse = lex_and_parse(src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_errors_eq(
        res.unwrap_err(),
        vec![CompilerErrorKind::ForeignKeyInconsistentNullability],
    );
}

#[test]
fn d1_model_fk_column_already_in_foreign_key() {
    // Arrange
    let src = with_env(
        r#"
        @d1(my_d1)
        model User {
            [primary id]
            id: int
            postId: int

            [foreign postId -> Post::id]
            [foreign postId -> Post::id] // same column in a second FK
        }

        @d1(my_d1)
        model Post {
            [primary id]
            id: int
        }
    "#,
    );
    let parse = lex_and_parse(&src);

    // Act
    let res = SemanticAnalysis::analyze(parse, &create_spec());

    // Assert
    assert_errors_eq(
        res.unwrap_err(),
        vec![CompilerErrorKind::ForeignKeyColumnAlreadyInForeignKey],
    );
}

// #[test]
// fn model_cycle_detection_error() {
//     // Arrange
//     let mut ast = create_ast(vec![
//         // A -> B -> C -> A
//         ModelBuilder::new("A")
//             .default_db()
//             .id_pk()
//             .col(
//                 "bId",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "B".into(),
//                     column_name: "id".into(),
//                 }),
//                 None,
//             )
//             .build(),
//         ModelBuilder::new("B")
//             .default_db()
//             .id_pk()
//             .col(
//                 "cId",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "C".into(),
//                     column_name: "id".into(),
//                 }),
//                 None,
//             )
//             .build(),
//         ModelBuilder::new("C")
//             .default_db()
//             .id_pk()
//             .col(
//                 "aId",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "A".into(),
//                     column_name: "id".into(),
//                 }),
//                 None,
//             )
//             .build(),
//     ]);
//     let spec = create_spec(&ast);

//     // Act
//     let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

//     // Assert
//     assert!(matches!(err.kind, GeneratorErrorKind::CyclicalDependency));
//     assert!(err.context.contains("A, B, C"));
// }

// #[test]
// fn service_cycle_detection_error() {
//     // Arrange
//     let mut ast = create_ast(vec![]);
//     let services = vec![
//         // A -> B -> C -> A
//         Service {
//             name: "A".into(),
//             attributes: vec![ServiceAttribute {
//                 var_name: "b".into(),
//                 inject_reference: "B".into(),
//             }],
//             initializer: None,
//             methods: BTreeMap::default(),
//             source_path: PathBuf::default(),
//         },
//         Service {
//             name: "B".into(),
//             attributes: vec![ServiceAttribute {
//                 var_name: "c".into(),
//                 inject_reference: "C".into(),
//             }],
//             initializer: None,
//             methods: BTreeMap::default(),
//             source_path: PathBuf::default(),
//         },
//         Service {
//             name: "C".into(),
//             attributes: vec![ServiceAttribute {
//                 var_name: "a".into(),
//                 inject_reference: "A".into(),
//             }],
//             initializer: None,
//             methods: BTreeMap::default(),
//             source_path: PathBuf::default(),
//         },
//     ];
//     ast.services = services.into_iter().map(|s| (s.name.clone(), s)).collect();

//     let spec = create_spec(&ast);

//     // Act
//     let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

//     // Assert
//     assert!(matches!(err.kind, GeneratorErrorKind::CyclicalDependency));
//     assert!(err.context.contains("A, B, C"));
// }

// #[test]
// fn model_attr_nullability_prevents_cycle_error() {
//     // Arrange
//     // A -> B -> C -> Nullable<A>
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("A")
//             .default_db()
//             .id_pk()
//             .col(
//                 "bId",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "B".into(),
//                     column_name: "id".into(),
//                 }),
//                 None,
//             )
//             .build(),
//         ModelBuilder::new("B")
//             .default_db()
//             .id_pk()
//             .col(
//                 "cId",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "C".into(),
//                     column_name: "id".into(),
//                 }),
//                 None,
//             )
//             .build(),
//         ModelBuilder::new("C")
//             .default_db()
//             .id_pk()
//             .col(
//                 "aId",
//                 CidlType::nullable(CidlType::Integer),
//                 Some(ForeignKey {
//                     model_name: "A".into(),
//                     column_name: "id".into(),
//                 }),
//                 None,
//             )
//             .build(),
//     ]);
//     let spec = create_spec(&ast);

//     // Act
//     SemanticAnalysis::analyze(&mut ast, &spec).expect("analysis to pass");
// }

// #[test]
// fn one_to_one_nav_property_unknown_attribute_reference_error() {
//     // Arrange
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("Dog").default_db().id_pk().build(),
//         ModelBuilder::new("Person")
//             .default_db()
//             .id_pk()
//             .nav_p(
//                 "dog",
//                 "Dog",
//                 NavigationPropertyKind::OneToOne {
//                     key_columns: vec!["dogId".to_string()], // dogId doesn't exist on Person
//                 },
//             )
//             .build(),
//     ]);
//     let spec = create_spec(&ast);

//     // Act
//     let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

//     // Assert
//     assert!(matches!(
//         err.kind,
//         GeneratorErrorKind::InvalidNavigationPropertyReference
//     ));
// }

// #[test]
// fn one_to_one_mismatched_fk_and_nav_type_error() {
//     // Arrange: attribute dogId references Dog, but nav prop type is Cat -> mismatch
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("Dog").default_db().id_pk().build(),
//         ModelBuilder::new("Cat").default_db().id_pk().build(),
//         ModelBuilder::new("Person")
//             .default_db()
//             .id_pk()
//             .col(
//                 "dogId",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "Dog".into(),
//                     column_name: "id".into(),
//                 }),
//                 None,
//             )
//             .nav_p(
//                 "dog",
//                 "Cat", // incorrect: says Cat but fk points to Dog
//                 NavigationPropertyKind::OneToOne {
//                     key_columns: vec!["dogId".to_string()],
//                 },
//             )
//             .build(),
//     ]);
//     let spec = create_spec(&ast);

//     // Act
//     let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

//     // Assert
//     assert!(matches!(
//         err.kind,
//         GeneratorErrorKind::InvalidNavigationPropertyReference
//     ));
// }

// #[test]
// fn one_to_many_unresolved_reference_error() {
//     // Arrange:
//     // Person declares OneToMany to Dog referencing Dog.personId, but Dog has no personId FK attr.
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("Dog").default_db().id_pk().build(), // no personId attribute
//         ModelBuilder::new("Person")
//             .default_db()
//             .id_pk()
//             .nav_p(
//                 "dogs",
//                 "Dog",
//                 NavigationPropertyKind::OneToMany {
//                     key_columns: vec!["personId".to_string()],
//                 },
//             )
//             .build(),
//     ]);
//     let spec = create_spec(&ast);

//     // Act
//     let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

//     // Assert
//     assert!(err.context.contains(
//         "Person.dogs references Dog.personId which does not exist or is not a foreign key to Person"
//     ));
// }

// #[test]
// fn junction_table_builder_errors() {
//     // Missing second nav property case: only one side of many-to-many
//     {
//         let mut ast = create_ast(vec![
//             ModelBuilder::new("Student")
//                 .default_db()
//                 .id_pk()
//                 .nav_p("courses", "Course", NavigationPropertyKind::ManyToMany)
//                 .build(),
//             // Course exists, but doesn't declare the reciprocal nav property
//             ModelBuilder::new("Course").default_db().id_pk().build(),
//         ]);
//         let spec = create_spec(&ast);

//         let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();
//         assert!(matches!(
//             err.kind,
//             GeneratorErrorKind::MissingManyToManyReference
//         ));
//     }

//     // Too many models case: two many-to-many nav properties pointing to the same model
//     {
//         let mut ast = create_ast(vec![
//             ModelBuilder::new("A")
//                 .default_db()
//                 .id_pk()
//                 .nav_p("bs", "B", NavigationPropertyKind::ManyToMany)
//                 .nav_p("bs2", "B", NavigationPropertyKind::ManyToMany)
//                 .build(),
//             ModelBuilder::new("B")
//                 .default_db()
//                 .id_pk()
//                 .nav_p("as", "A", NavigationPropertyKind::ManyToMany)
//                 .build(),
//         ]);
//         let spec = create_spec(&ast);

//         let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();
//         assert!(matches!(
//             err.kind,
//             GeneratorErrorKind::ExtraneousManyToManyReferences
//         ));
//     }
// }

// #[test]
// fn instantiated_stream_method() {
//     // Arrange
//     let model = ModelBuilder::new("Dog")
//         .default_db()
//         .id_pk()
//         .method(
//             "uploadPhoto",
//             HttpVerb::Post,
//             false,
//             vec![
//                 Field {
//                     name: "stream".into(),
//                     cidl_type: CidlType::Stream,
//                 },
//                 Field {
//                     name: "ds".into(),
//                     cidl_type: CidlType::DataSource("Dog".into()),
//                 },
//             ],
//             CidlType::Stream,
//             None,
//         )
//         .build();

//     let mut ast = create_ast(vec![model]);
//     let spec = create_spec(&ast);

//     // Act
//     let res = SemanticAnalysis::analyze(&mut ast, &spec);

//     // Assert
//     res.unwrap();
// }

// #[test]
// fn static_stream_method() {
//     // Arrange
//     let model = ModelBuilder::new("Dog")
//         .default_db()
//         .id_pk()
//         .method(
//             "uploadPhoto",
//             HttpVerb::Post,
//             true,
//             vec![Field {
//                 name: "stream".into(),
//                 cidl_type: CidlType::Stream,
//             }],
//             CidlType::Stream,
//             None,
//         )
//         .build();

//     let mut ast = create_ast(vec![model]);
//     let spec = create_spec(&ast);

//     // Act
//     let res = SemanticAnalysis::analyze(&mut ast, &spec);

//     // Assert
//     res.unwrap();
// }

// #[test]
// fn invalid_stream_method() {
//     // Arrange
//     let model = ModelBuilder::new("Dog")
//         .default_db()
//         .id_pk()
//         .method(
//             "uploadPhoto",
//             HttpVerb::Post,
//             true, // static is true, can only have 1 param
//             vec![
//                 Field {
//                     name: "stream".into(),
//                     cidl_type: CidlType::Stream,
//                 },
//                 Field {
//                     name: "id".into(),
//                     cidl_type: CidlType::Integer,
//                 },
//             ],
//             CidlType::Stream,
//             None,
//         )
//         .build();

//     let mut ast = create_ast(vec![model]);
//     let spec = create_spec(&ast);

//     // Act
//     let res = SemanticAnalysis::analyze(&mut ast, &spec);

//     // Assert
//     assert!(matches!(
//         res.unwrap_err().kind,
//         GeneratorErrorKind::InvalidStream
//     ));
// }

// #[test]
// fn missing_variable_in_wrangler() {
//     // Arrange
//     let mut ast = create_ast(vec![ModelBuilder::new("User").db("my_d1").id_pk().build()]);
//     ast.wrangler_env = Some(WranglerEnv {
//         name: "Env".into(),
//         source_path: "source.ts".into(),
//         d1_bindings: vec!["my_d1".into()],
//         kv_bindings: vec![],
//         r2_bindings: vec![],
//         vars: [
//             ("API_KEY".into(), ast::CidlType::Text),
//             ("TIMEOUT".into(), ast::CidlType::Integer),
//         ]
//         .into_iter()
//         .collect(),
//     });

//     let specs = vec![
//         WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
//         WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
//     ];

//     for spec in specs {
//         // Act
//         let res = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind;

//         // Assert
//         assert!(matches!(
//             res,
//             GeneratorErrorKind::InconsistentWranglerBinding
//         ));
//     }
// }

// #[test]
// fn missing_env_in_ast() {
//     // Arrange
//     let mut ast = create_ast(vec![ModelBuilder::new("User").id_pk().build()]);
//     ast.wrangler_env = None;

//     let specs = vec![
//         WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
//         WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
//     ];

//     for spec in specs {
//         // Act
//         let res = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind;

//         // Assert
//         assert!(matches!(res, GeneratorErrorKind::MissingWranglerEnv));
//     }
// }

// #[test]
// fn missing_d1_binding_in_wrangler() {
//     // Arrange
//     let mut ast = create_ast(vec![ModelBuilder::new("User").default_db().id_pk().build()]);

//     let specs = vec![
//         WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
//         WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
//     ];

//     for spec in specs {
//         // Act
//         let res = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind;

//         // Assert
//         assert!(matches!(
//             res,
//             GeneratorErrorKind::InconsistentWranglerBinding
//         ));
//     }
// }

// #[test]
// fn missing_kv_bindings_in_wrangler() {
//     // Arrange
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("Settings")
//             .kv_object(
//                 "settings",
//                 "my_binding",
//                 "settings",
//                 false,
//                 CidlType::JsonValue,
//             )
//             .build(),
//     ]);

//     let specs = vec![
//         WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec(),
//         WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec(),
//     ];

//     for spec in specs {
//         // Act
//         let res = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err().kind;

//         // Assert
//         assert!(matches!(
//             res,
//             GeneratorErrorKind::InconsistentWranglerBinding
//         ));
//     }
// }

// #[test]
// fn kv_object_valid_key_format() {
//     // Arrange
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("Settings")
//             .db("my_d1")
//             .id_pk()
//             .col("userId", CidlType::Integer, None, None)
//             .key_param("tenant")
//             .kv_object(
//                 "settings/{tenant}/{userId}",
//                 "my_kv",
//                 "config",
//                 false,
//                 CidlType::JsonValue,
//             )
//             .build(),
//     ]);
//     ast.wrangler_env = Some(WranglerEnv {
//         name: "Env".into(),
//         source_path: "source.ts".into(),
//         d1_bindings: vec!["my_d1".into()],
//         kv_bindings: vec!["my_kv".into()],
//         r2_bindings: vec![],
//         vars: HashMap::new(),
//     });

//     let spec = create_spec(&ast);

//     // Act
//     let result = SemanticAnalysis::analyze(&mut ast, &spec);

//     // Assert
//     result.expect("analysis to pass");
// }

// #[test]
// fn kv_object_missing_key_param_error() {
//     // Arrange
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("Settings")
//             .db("my_d1")
//             .id_pk()
//             .kv_object(
//                 "settings/{tenant}/{userId}",
//                 "my_kv",
//                 "config",
//                 false,
//                 CidlType::JsonValue,
//             )
//             .build(),
//     ]);
//     ast.wrangler_env = Some(WranglerEnv {
//         name: "Env".into(),
//         source_path: "source.ts".into(),
//         d1_bindings: vec!["my_d1".into()],
//         kv_bindings: vec!["my_kv".into()],
//         r2_bindings: vec![],
//         vars: HashMap::new(),
//     });
//     let spec = create_spec(&ast);

//     // Act
//     let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

//     // Assert
//     assert!(matches!(err.kind, GeneratorErrorKind::UnknownKeyReference));
// }

// #[test]
// fn kv_object_with_primary_key_in_format() {
//     // Arrange
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("User")
//             .db("my_d1")
//             .id_pk()
//             .kv_object(
//                 "user/{id}/preferences",
//                 "my_kv",
//                 "prefs",
//                 false,
//                 CidlType::JsonValue,
//             )
//             .build(),
//     ]);
//     ast.wrangler_env = Some(WranglerEnv {
//         name: "Env".into(),
//         source_path: "source.ts".into(),
//         d1_bindings: vec!["my_d1".into()],
//         kv_bindings: vec!["my_kv".into()],
//         r2_bindings: vec![],
//         vars: HashMap::new(),
//     });
//     let spec = create_spec(&ast);

//     // Act
//     let result = SemanticAnalysis::analyze(&mut ast, &spec);

//     // Assert
//     result.expect("analysis to pass");
// }

// #[test]
// fn kv_object_with_column_in_format() {
//     // Arrange
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("Session")
//             .id_pk()
//             .col("token", CidlType::Text, None, None)
//             .kv_object(
//                 "session/{token}",
//                 "my_kv",
//                 "data",
//                 false,
//                 CidlType::JsonValue,
//             )
//             .build(),
//     ]);
//     ast.wrangler_env = Some(WranglerEnv {
//         name: "Env".into(),
//         source_path: "source.ts".into(),
//         d1_bindings: vec!["my_d1".into()],
//         kv_bindings: vec!["my_kv".into()],
//         r2_bindings: vec![],
//         vars: HashMap::new(),
//     });
//     let spec = create_spec(&ast);

//     // Act
//     let result = SemanticAnalysis::analyze(&mut ast, &spec);

//     // Assert
//     result.expect("analysis to pass");
// }

// #[test]
// fn kv_object_invalid_nested_braces_error() {
//     // Arrange
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("Settings")
//             .db("my_d1")
//             .id_pk()
//             .kv_object(
//                 "settings/{{nested}}",
//                 "my_kv",
//                 "config",
//                 false,
//                 CidlType::JsonValue,
//             )
//             .build(),
//     ]);
//     ast.wrangler_env = Some(WranglerEnv {
//         name: "Env".into(),
//         source_path: "source.ts".into(),
//         d1_bindings: vec!["my_d1".into()],
//         kv_bindings: vec!["my_kv".into()],
//         r2_bindings: vec![],
//         vars: HashMap::new(),
//     });
//     let spec = create_spec(&ast);

//     // Act
//     let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

//     // Assert
//     assert!(matches!(err.kind, GeneratorErrorKind::InvalidKeyFormat));
// }

// #[test]
// fn kv_object_unclosed_brace_error() {
//     // Arrange
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("Settings")
//             .db("my_d1")
//             .id_pk()
//             .kv_object(
//                 "settings/{unclosed",
//                 "my_kv",
//                 "config",
//                 false,
//                 CidlType::JsonValue,
//             )
//             .build(),
//     ]);
//     ast.wrangler_env = Some(WranglerEnv {
//         name: "Env".into(),
//         source_path: "source.ts".into(),
//         d1_bindings: vec!["my_d1".into()],
//         kv_bindings: vec!["my_kv".into()],
//         r2_bindings: vec![],
//         vars: HashMap::new(),
//     });
//     let spec = create_spec(&ast);

//     // Act
//     let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

//     // Assert
//     assert!(matches!(err.kind, GeneratorErrorKind::InvalidKeyFormat));
// }

// #[test]
// fn http_result_stream_return_type() {
//     // Arrange
//     let model = ModelBuilder::new("Dog")
//         .id_pk()
//         .method(
//             "downloadPhoto",
//             HttpVerb::Get,
//             true,
//             vec![Field {
//                 name: "ds".into(),
//                 cidl_type: CidlType::DataSource("Dog".into()),
//             }],
//             CidlType::http(CidlType::Stream),
//             None,
//         )
//         .build();

//     let mut ast = create_ast(vec![model]);
//     let spec = create_spec(&ast);

//     // Act
//     let res = SemanticAnalysis::analyze(&mut ast, &spec);

//     // Assert
//     res.unwrap();
// }

// #[test]
// fn data_source_valid_instance_method() {
//     // Arrange
//     let model = ModelBuilder::new("Dog")
//         .id_pk()
//         .data_source("dogs", IncludeTreeBuilder::default().build(), false)
//         .method(
//             "getDogs",
//             HttpVerb::Get,
//             false, // instance method
//             vec![],
//             CidlType::Object("Dog".into()),
//             Some("dogs".into()),
//         )
//         .build();

//     let mut ast = create_ast(vec![model]);
//     let spec = create_spec(&ast);

//     // Act
//     let result = SemanticAnalysis::analyze(&mut ast, &spec);

//     // Assert
//     result.expect("analysis to pass with valid data source");
// }

// #[test]
// fn data_source_on_static_method() {
//     // Arrange
//     let model = ModelBuilder::new("Dog")
//         .id_pk()
//         .data_source("dogs", IncludeTreeBuilder::default().build(), false)
//         .method(
//             "getDogs",
//             HttpVerb::Get,
//             true,
//             vec![],
//             CidlType::Object("Dog".into()),
//             Some("dogs".into()),
//         )
//         .build();

//     let mut ast = create_ast(vec![model]);
//     let spec = create_spec(&ast);

//     // Act
//     let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

//     // Assert
//     assert!(matches!(
//         err.kind,
//         GeneratorErrorKind::InvalidDataSourceReference
//     ));
//     assert!(err.context.contains("static method"));
// }

// #[test]
// fn data_source_unknown() {
//     // Arrange
//     let model = ModelBuilder::new("Dog")
//         .id_pk()
//         .method(
//             "getDogs",
//             HttpVerb::Get,
//             false,
//             vec![],
//             CidlType::Object("Dog".into()),
//             Some("nonexistent".into()), // This data source doesn't exist
//         )
//         .build();

//     let mut ast = create_ast(vec![model]);
//     let spec = create_spec(&ast);

//     // Act
//     let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();

//     // Assert
//     assert!(matches!(
//         err.kind,
//         GeneratorErrorKind::UnknownDataSourceReference
//     ));
// }

// #[test]
// fn composite_key_validation() {
//     // Valid composite key: Enrollment has a composite key referencing Student(id, id2)
//     {
//         let mut ast = create_ast(vec![
//             ModelBuilder::new("Student")
//                 .default_db()
//                 .pk("id", CidlType::Integer)
//                 .pk("id2", CidlType::Integer)
//                 .build(),
//             ModelBuilder::new("Enrollment")
//                 .default_db()
//                 .id_pk()
//                 .col(
//                     "studentId",
//                     CidlType::Integer,
//                     Some(ForeignKey {
//                         model_name: "Student".into(),
//                         column_name: "id".into(),
//                     }),
//                     Some(1),
//                 )
//                 .col(
//                     "studentId2",
//                     CidlType::Integer,
//                     Some(ForeignKey {
//                         model_name: "Student".into(),
//                         column_name: "id2".into(),
//                     }),
//                     Some(1),
//                 )
//                 .col("grade", CidlType::Text, None, None)
//                 .build(),
//         ]);
//         let spec = create_spec(&ast);
//         SemanticAnalysis::analyze(&mut ast, &spec).expect("valid composite key should pass");
//     }

//     // Error: composite key with only 1 column
//     {
//         let mut ast = create_ast(vec![
//             ModelBuilder::new("Student").default_db().id_pk().build(),
//             ModelBuilder::new("Enrollment")
//                 .default_db()
//                 .id_pk()
//                 .col(
//                     "studentId",
//                     CidlType::Integer,
//                     Some(ForeignKey {
//                         model_name: "Student".into(),
//                         column_name: "id".into(),
//                     }),
//                     Some(1),
//                 )
//                 .build(),
//         ]);
//         let spec = create_spec(&ast);
//         let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();
//         assert!(matches!(err.kind, GeneratorErrorKind::InvalidCompositeKey));
//         assert!(err.context.contains("at least 2 columns"));
//     }

//     // Error: composite key contains non-FK column
//     {
//         let mut ast = create_ast(vec![
//             ModelBuilder::new("Student").default_db().id_pk().build(),
//             ModelBuilder::new("Enrollment")
//                 .default_db()
//                 .id_pk()
//                 .col(
//                     "studentId",
//                     CidlType::Integer,
//                     Some(ForeignKey {
//                         model_name: "Student".into(),
//                         column_name: "id".into(),
//                     }),
//                     Some(1),
//                 )
//                 .col("grade", CidlType::Text, None, Some(1))
//                 .build(),
//         ]);
//         let spec = create_spec(&ast);
//         let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();
//         assert!(matches!(err.kind, GeneratorErrorKind::InvalidCompositeKey));
//         assert!(err.context.contains("not a foreign key"));
//     }

//     // Error: composite key with nav prop doesn't reference all PK columns
//     {
//         let mut ast = create_ast(vec![
//             ModelBuilder::new("Student")
//                 .default_db()
//                 .pk("id", CidlType::Integer)
//                 .pk("id2", CidlType::Integer)
//                 .build(),
//             ModelBuilder::new("Enrollment")
//                 .default_db()
//                 .id_pk()
//                 .col(
//                     "studentId",
//                     CidlType::Integer,
//                     Some(ForeignKey {
//                         model_name: "Student".into(),
//                         column_name: "id".into(),
//                     }),
//                     Some(1),
//                 )
//                 .col(
//                     "studentIdDup",
//                     CidlType::Integer,
//                     Some(ForeignKey {
//                         model_name: "Student".into(),
//                         column_name: "id2".into(),
//                     }),
//                     Some(1),
//                 )
//                 .nav_p(
//                     "student",
//                     "Student",
//                     NavigationPropertyKind::OneToOne {
//                         key_columns: vec!["studentId".to_string()], // missing studentIdDup
//                     },
//                 )
//                 .build(),
//         ]);
//         let spec = create_spec(&ast);
//         let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();
//         assert!(matches!(
//             err.kind,
//             GeneratorErrorKind::InvalidNavigationPropertyReference
//         ));
//     }

//     // Error: composite key with mixed nullability
//     {
//         let mut ast = create_ast(vec![
//             ModelBuilder::new("Student")
//                 .default_db()
//                 .pk("id", CidlType::Integer)
//                 .pk("id2", CidlType::Integer)
//                 .build(),
//             ModelBuilder::new("Enrollment")
//                 .default_db()
//                 .id_pk()
//                 .col(
//                     "studentId",
//                     CidlType::Integer, // Non-nullable
//                     Some(ForeignKey {
//                         model_name: "Student".into(),
//                         column_name: "id".into(),
//                     }),
//                     Some(1),
//                 )
//                 .col(
//                     "studentId2",
//                     CidlType::nullable(CidlType::Integer), // Nullable
//                     Some(ForeignKey {
//                         model_name: "Student".into(),
//                         column_name: "id2".into(),
//                     }),
//                     Some(1),
//                 )
//                 .build(),
//         ]);
//         let spec = create_spec(&ast);
//         let err = SemanticAnalysis::analyze(&mut ast, &spec).unwrap_err();
//         assert!(matches!(err.kind, GeneratorErrorKind::InvalidCompositeKey));
//     }
// }

// #[test]
// fn multiple_composite_keys_same_model() {
//     // Arrange
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("User")
//             .default_db()
//             .pk("id", CidlType::Integer)
//             .pk("id2", CidlType::Integer)
//             .build(),
//         ModelBuilder::new("Group")
//             .default_db()
//             .pk("id", CidlType::Integer)
//             .pk("id2", CidlType::Integer)
//             .build(),
//         ModelBuilder::new("Permission")
//             .default_db()
//             .id_pk()
//             .col(
//                 "userId",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "User".into(),
//                     column_name: "id".into(),
//                 }),
//                 Some(1), // composite_id = 1
//             )
//             .col(
//                 "userId2",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "User".into(),
//                     column_name: "id2".into(),
//                 }),
//                 Some(1),
//             )
//             .col(
//                 "groupId",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "Group".into(),
//                     column_name: "id".into(),
//                 }),
//                 Some(2), // composite_id = 2 (different composite key)
//             )
//             .col(
//                 "groupId2",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "Group".into(),
//                     column_name: "id2".into(),
//                 }),
//                 Some(2),
//             )
//             .build(),
//     ]);
//     let spec = create_spec(&ast);

//     // Act
//     let result = SemanticAnalysis::analyze(&mut ast, &spec);

//     // Assert
//     result.expect("multiple composite keys should be valid");
// }
