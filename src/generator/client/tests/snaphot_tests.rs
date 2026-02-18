use std::{collections::BTreeMap, path::PathBuf};

use ast::{
    ApiMethod, CidlType, CrudKind, HttpVerb, MediaType, NamedTypedValue, PlainOldObject, Service,
    ServiceAttribute,
};
use client::ClientGenerator;
use generator_test::{ModelBuilder, create_ast, create_spec};
use semantic::SemanticAnalysis;
use workers::WorkersGenerator;

/**
 * Snapshot tests for client code generation.
 *
 * Note that the regression tests (cloesce/tests/regression) also cover client code generation.
 */
#[test]
fn test_client_code_generation_snapshot() {
    let mut ast = create_ast(vec![
        ModelBuilder::new("BasicModel")
            .id_pk()
            .col(
                "fk_to_model",
                CidlType::Integer,
                Some("OneToManyModel".into()),
            )
            .build(),
        // All valid SQL column types
        ModelBuilder::new("HasSqlColumnTypes")
            .id_pk()
            .col("string", CidlType::Text, None)
            .col("integer", CidlType::Integer, None)
            .col("real", CidlType::Real, None)
            .col("boolean", CidlType::Boolean, None)
            .col("date", CidlType::DateIso, None)
            .col("stringNull", CidlType::nullable(CidlType::Text), None)
            .col("integerNull", CidlType::nullable(CidlType::Integer), None)
            .col("realNull", CidlType::nullable(CidlType::Real), None)
            .col("booleanNull", CidlType::nullable(CidlType::Boolean), None)
            .col("dateNull", CidlType::nullable(CidlType::DateIso), None)
            .build(),
        // One to One Navigation Property
        ModelBuilder::new("HasOneToOne")
            .id_pk()
            // one to one
            .col("basicModelId", CidlType::Integer, Some("BasicModel".into()))
            .nav_p(
                "oneToOneNav",
                "BasicModel",
                ast::NavigationPropertyKind::OneToOne {
                    column_reference: "basicModelId".into(),
                },
            )
            .build(),
        // One to Many Navigation Property
        ModelBuilder::new("OneToManyModel")
            .id_pk()
            .nav_p(
                "oneToManyNav",
                "BasicModel",
                ast::NavigationPropertyKind::OneToMany {
                    column_reference: "fk_to_model".into(),
                },
            )
            .build(),
        // Many to Many
        ModelBuilder::new("ManyToManyModelA")
            .id_pk()
            .nav_p(
                "manyToManyNav",
                "ManyToManyModelB",
                ast::NavigationPropertyKind::ManyToMany,
            )
            .build(),
        ModelBuilder::new("ManyToManyModelB")
            .id_pk()
            .nav_p(
                "manyToManyNav",
                "ManyToManyModelA",
                ast::NavigationPropertyKind::ManyToMany,
            )
            .build(),
        // KV
        ModelBuilder::new("ModelWithKv")
            .key_param("id1")
            .key_param("id2")
            .kv_object(
                "{id1}",
                "kv_namespace",
                "someValue",
                false,
                CidlType::JsonValue,
            )
            .kv_object("", "kv_namespace", "manyValues", true, CidlType::JsonValue)
            .kv_object(
                "constant",
                "kv_namespace",
                "streamValue",
                false,
                CidlType::Stream,
            )
            .method(
                "instanceMethod",
                HttpVerb::POST,
                false,
                vec![NamedTypedValue {
                    name: "input".into(),
                    cidl_type: CidlType::Text,
                }],
                CidlType::Text,
            )
            .method(
                "staticMethod",
                HttpVerb::GET,
                true,
                vec![NamedTypedValue {
                    name: "input".into(),
                    cidl_type: CidlType::Integer,
                }],
                CidlType::Integer,
            )
            .method(
                "hasKvParamAndRes",
                HttpVerb::POST,
                false,
                vec![NamedTypedValue {
                    name: "input".into(),
                    cidl_type: CidlType::KvObject(Box::new(CidlType::Text)),
                }],
                CidlType::KvObject(Box::new(CidlType::Text)),
            )
            .build(),
        // R2
        ModelBuilder::new("ModelWithR2")
            .id_pk()
            .key_param("r2Id")
            .r2_object("r2/{id}/{r2Id}", "r2_namespace", "fileData", false)
            .r2_object("r2", "r2_namespace", "manyFileDatas", true)
            .method(
                "hasR2ParamAndRes",
                HttpVerb::POST,
                false,
                vec![NamedTypedValue {
                    name: "input".into(),
                    cidl_type: CidlType::R2Object,
                }],
                CidlType::R2Object,
            )
            .build(),
        // Hybrid (D1, KV, R2)
        ModelBuilder::new("ToyotaPrius")
            .id_pk()
            .col("modelYear", CidlType::Integer, None)
            .key_param("ownerId")
            .key_param("vehicleId")
            .kv_object(
                "{ownerId}/{modelYear}",
                "owner_metadata",
                "metadata",
                false,
                CidlType::JsonValue,
            )
            .r2_object("{vehicleId}", "vehicle_photos", "photoData", false)
            .method(
                "instanceMethod",
                HttpVerb::POST,
                false,
                vec![NamedTypedValue {
                    name: "input".into(),
                    cidl_type: CidlType::Text,
                }],
                CidlType::Text,
            )
            .build(),
    ]);

    // CRUD methods
    {
        let mut model_with_cruds = ModelBuilder::new("ModelWithCruds")
            .id_pk()
            .col("name", CidlType::Text, None)
            .build();
        model_with_cruds.cruds.push(CrudKind::GET);
        model_with_cruds.cruds.push(CrudKind::SAVE);
        model_with_cruds.cruds.push(CrudKind::LIST);
        ast.models
            .insert(model_with_cruds.name.clone(), model_with_cruds);
    }

    // services + stream methods
    {
        let mut methods = BTreeMap::new();
        methods.insert(
            "staticMethod".into(),
            ApiMethod {
                name: "staticMethod".into(),
                is_static: true,
                http_verb: HttpVerb::GET,
                return_type: CidlType::http(CidlType::Text),
                parameters_media: MediaType::default(),
                parameters: vec![NamedTypedValue {
                    name: "input".into(),
                    cidl_type: CidlType::Text,
                }],
                return_media: MediaType::default(),
            },
        );
        methods.insert(
            "instanceMethod".into(),
            ApiMethod {
                name: "instanceMethod".into(),
                is_static: false,
                http_verb: HttpVerb::POST,
                return_type: CidlType::http(CidlType::Integer),
                parameters_media: MediaType::default(),
                parameters: vec![NamedTypedValue {
                    name: "input".into(),
                    cidl_type: CidlType::Integer,
                }],
                return_media: MediaType::default(),
            },
        );

        // Intake stream
        methods.insert(
            "uploadData".into(),
            ApiMethod {
                name: "uploadData".into(),
                is_static: false,
                http_verb: HttpVerb::POST,
                return_type: CidlType::http(CidlType::Boolean),
                parameters_media: ast::MediaType::Octet,
                parameters: vec![NamedTypedValue {
                    name: "data".into(),
                    cidl_type: CidlType::Stream,
                }],
                return_media: ast::MediaType::default(),
            },
        );

        // Output stream
        methods.insert(
            "downloadData".into(),
            ApiMethod {
                name: "downloadData".into(),
                is_static: false,
                http_verb: HttpVerb::GET,
                return_type: CidlType::Stream,
                parameters_media: MediaType::default(),
                parameters: vec![],
                return_media: ast::MediaType::Octet,
            },
        );

        ast.services.insert(
            "BasicService".into(),
            Service {
                name: "BasicService".into(),
                attributes: vec![ServiceAttribute {
                    var_name: "db".into(),
                    inject_reference: "D1Database".into(),
                }],
                initializer: None,
                methods,
                source_path: PathBuf::default(),
            },
        );
    }

    // plain old objects
    {
        ast.poos.insert(
            "BasicPoo".into(),
            PlainOldObject {
                name: "BasicPoo".into(),
                attributes: vec![
                    NamedTypedValue {
                        name: "field1".into(),
                        cidl_type: CidlType::Text,
                    },
                    NamedTypedValue {
                        name: "field2".into(),
                        cidl_type: CidlType::Integer,
                    },
                ],
                source_path: PathBuf::default(),
            },
        );

        ast.poos.insert(
            "PooWithComposition".into(),
            PlainOldObject {
                name: "PooWithComposition".into(),
                attributes: vec![
                    NamedTypedValue {
                        name: "field1".into(),
                        cidl_type: CidlType::Object("BasicPoo".into()),
                    },
                    NamedTypedValue {
                        name: "field2".into(),
                        cidl_type: CidlType::Object("BasicModel".into()),
                    },
                ],
                source_path: PathBuf::default(),
            },
        );
    }

    let spec = create_spec(&ast);
    SemanticAnalysis::analyze(&mut ast, &spec).expect("Semantic analysis to pass");
    WorkersGenerator::finalize_api_methods(&mut ast);

    let client_code = ClientGenerator::generate(&ast, "http://example.com/api".to_string());

    insta::assert_snapshot!("client_code_generation_snapshot", client_code);
}
