// use std::{collections::BTreeMap, path::PathBuf};

// use ast::{
//     ApiMethod, CidlType, CrudKind, ForeignKey, HttpVerb, IncludeTree, MediaType,
//     Field, PlainOldObject, Service, ServiceAttribute,
// };
// use client::ClientGenerator;
// use generator_test::{IncludeTreeBuilder, ModelBuilder, create_ast, create_spec};
// use semantic::SemanticAnalysis;
// use workers::WorkersGenerator;

// /**
//  * Snapshot tests for client code generation.
//  *
//  * Note that the regression tests (cloesce/tests/regression) also cover client code generation.
//  */
// #[test]
// fn test_client_code_generation_snapshot() {
//     let mut ast = create_ast(vec![
//         ModelBuilder::new("BasicModel")
//             .default_db()
//             .id_pk()
//             .col(
//                 "fk_to_model",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "OneToManyModel".into(),
//                     column_name: "id".into(),
//                 }),
//                 None,
//             )
//             .build(),
//         // All valid SQL column types
//         ModelBuilder::new("HasSqlColumnTypes")
//             .default_db()
//             .id_pk()
//             .col("string", CidlType::Text, None, None)
//             .col("integer", CidlType::Integer, None, None)
//             .col("real", CidlType::Real, None, None)
//             .col("boolean", CidlType::Boolean, None, None)
//             .col("date", CidlType::DateIso, None, None)
//             .col("stringNull", CidlType::nullable(CidlType::Text), None, None)
//             .col(
//                 "integerNull",
//                 CidlType::nullable(CidlType::Integer),
//                 None,
//                 None,
//             )
//             .col("realNull", CidlType::nullable(CidlType::Real), None, None)
//             .col(
//                 "booleanNull",
//                 CidlType::nullable(CidlType::Boolean),
//                 None,
//                 None,
//             )
//             .col(
//                 "dateNull",
//                 CidlType::nullable(CidlType::DateIso),
//                 None,
//                 None,
//             )
//             .build(),
//         // One to One Navigation Property
//         ModelBuilder::new("HasOneToOne")
//             .default_db()
//             .id_pk()
//             // one to one
//             .col(
//                 "basicModelId",
//                 CidlType::Integer,
//                 Some(ForeignKey {
//                     model_name: "BasicModel".into(),
//                     column_name: "id".into(),
//                 }),
//                 None,
//             )
//             .nav_p(
//                 "oneToOneNav",
//                 "BasicModel",
//                 ast::NavigationPropertyKind::OneToOne {
//                     key_columns: vec!["id".into()],
//                 },
//             )
//             .build(),
//         // One to Many Navigation Property
//         ModelBuilder::new("OneToManyModel")
//             .default_db()
//             .id_pk()
//             .nav_p(
//                 "oneToManyNav",
//                 "BasicModel",
//                 ast::NavigationPropertyKind::OneToMany {
//                     key_columns: vec!["fk_to_model".into()],
//                 },
//             )
//             .build(),
//         // Many to Many
//         ModelBuilder::new("ManyToManyModelA")
//             .default_db()
//             .id_pk()
//             .nav_p(
//                 "manyToManyNav",
//                 "ManyToManyModelB",
//                 ast::NavigationPropertyKind::ManyToMany,
//             )
//             .build(),
//         ModelBuilder::new("ManyToManyModelB")
//             .default_db()
//             .id_pk()
//             .nav_p(
//                 "manyToManyNav",
//                 "ManyToManyModelA",
//                 ast::NavigationPropertyKind::ManyToMany,
//             )
//             .build(),
//         // Composite PK model
//         ModelBuilder::new("ModelWithCompositePk")
//             .default_db()
//             .pk("tenantId", CidlType::Text)
//             .pk("rowId", CidlType::Integer)
//             .col("name", CidlType::Text, None, None)
//             .method(
//                 "instanceMethod",
//                 HttpVerb::Post,
//                 false,
//                 vec![Field {
//                     name: "input".into(),
//                     cidl_type: CidlType::Text,
//                 }],
//                 CidlType::Text,
//                 None,
//             )
//             .build(),
//         // KV
//         ModelBuilder::new("ModelWithKv")
//             .key_param("id1")
//             .key_param("id2")
//             .kv_object("{id1}", "kv", "someValue", false, CidlType::JsonValue)
//             .kv_object("", "kv", "manyValues", true, CidlType::JsonValue)
//             .kv_object("constant", "kv", "streamValue", false, CidlType::Stream)
//             .method(
//                 "instanceMethod",
//                 HttpVerb::Post,
//                 false,
//                 vec![Field {
//                     name: "input".into(),
//                     cidl_type: CidlType::Text,
//                 }],
//                 CidlType::Text,
//                 None,
//             )
//             .method(
//                 "staticMethod",
//                 HttpVerb::Get,
//                 true,
//                 vec![Field {
//                     name: "input".into(),
//                     cidl_type: CidlType::Integer,
//                 }],
//                 CidlType::Integer,
//                 None,
//             )
//             .method(
//                 "hasKvParamAndRes",
//                 HttpVerb::Post,
//                 false,
//                 vec![Field {
//                     name: "input".into(),
//                     cidl_type: CidlType::KvObject(Box::new(CidlType::Text)),
//                 }],
//                 CidlType::KvObject(Box::new(CidlType::Text)),
//                 None,
//             )
//             .build(),
//         // R2
//         ModelBuilder::new("ModelWithR2")
//             .default_db()
//             .id_pk()
//             .key_param("r2Id")
//             .r2_object("r2/{id}/{r2Id}", "r2", "fileData", false)
//             .r2_object("r2", "r2", "manyFileDatas", true)
//             .method(
//                 "hasR2ParamAndRes",
//                 HttpVerb::Post,
//                 false,
//                 vec![Field {
//                     name: "input".into(),
//                     cidl_type: CidlType::R2Object,
//                 }],
//                 CidlType::R2Object,
//                 None,
//             )
//             .build(),
//         // Hybrid (D1, KV, R2)
//         ModelBuilder::new("ToyotaPrius")
//             .default_db()
//             .id_pk()
//             .col("modelYear", CidlType::Integer, None, None)
//             .key_param("ownerId")
//             .key_param("vehicleId")
//             .kv_object(
//                 "{ownerId}/{modelYear}",
//                 "kv",
//                 "metadata",
//                 false,
//                 CidlType::JsonValue,
//             )
//             .r2_object("{vehicleId}", "r2", "photoData", false)
//             .method(
//                 "instanceMethod",
//                 HttpVerb::Post,
//                 false,
//                 vec![Field {
//                     name: "input".into(),
//                     cidl_type: CidlType::Text,
//                 }],
//                 CidlType::Text,
//                 None,
//             )
//             .data_source(
//                 "withKV",
//                 IncludeTreeBuilder::default().add_node("metadata").build(),
//                 false,
//             )
//             .data_source(
//                 "withR2",
//                 IncludeTreeBuilder::default().add_node("photoData").build(),
//                 false,
//             )
//             .data_source("private", IncludeTree::default(), true)
//             .build(),
//     ]);

//     ast.models
//         .get_mut("HasOneToOne")
//         .unwrap()
//         .primary_key_columns
//         .first_mut()
//         .unwrap()
//         .foreign_key_reference = Some(ForeignKey {
//         model_name: "BasicModel".into(),
//         column_name: "id".into(),
//     });

//     // CRUD methods
//     {
//         let mut model_with_cruds = ModelBuilder::new("ModelWithCruds")
//             .default_db()
//             .id_pk()
//             .col("name", CidlType::Text, None, None)
//             .build();
//         model_with_cruds.cruds.push(CrudKind::GET);
//         model_with_cruds.cruds.push(CrudKind::SAVE);
//         model_with_cruds.cruds.push(CrudKind::LIST);
//         ast.models
//             .insert(model_with_cruds.name.clone(), model_with_cruds);
//     }

//     // services + stream methods
//     {
//         let mut methods = BTreeMap::new();
//         methods.insert(
//             "staticMethod".into(),
//             ApiMethod {
//                 name: "staticMethod".into(),
//                 is_static: true,
//                 http_verb: HttpVerb::Get,
//                 return_type: CidlType::http(CidlType::Text),
//                 parameters_media: MediaType::default(),
//                 parameters: vec![Field {
//                     name: "input".into(),
//                     cidl_type: CidlType::Text,
//                 }],
//                 return_media: MediaType::default(),
//                 data_source: None,
//             },
//         );
//         methods.insert(
//             "instanceMethod".into(),
//             ApiMethod {
//                 name: "instanceMethod".into(),
//                 is_static: false,
//                 http_verb: HttpVerb::Post,
//                 return_type: CidlType::http(CidlType::Integer),
//                 parameters_media: MediaType::default(),
//                 parameters: vec![Field {
//                     name: "input".into(),
//                     cidl_type: CidlType::Integer,
//                 }],
//                 return_media: MediaType::default(),
//                 data_source: None,
//             },
//         );

//         // Intake stream
//         methods.insert(
//             "uploadData".into(),
//             ApiMethod {
//                 name: "uploadData".into(),
//                 is_static: false,
//                 http_verb: HttpVerb::Post,
//                 return_type: CidlType::http(CidlType::Boolean),
//                 parameters_media: ast::MediaType::Octet,
//                 parameters: vec![Field {
//                     name: "data".into(),
//                     cidl_type: CidlType::Stream,
//                 }],
//                 return_media: ast::MediaType::default(),
//                 data_source: None,
//             },
//         );

//         // Output stream
//         methods.insert(
//             "downloadData".into(),
//             ApiMethod {
//                 name: "downloadData".into(),
//                 is_static: false,
//                 http_verb: HttpVerb::Get,
//                 return_type: CidlType::Stream,
//                 parameters_media: MediaType::default(),
//                 parameters: vec![],
//                 return_media: ast::MediaType::Octet,
//                 data_source: None,
//             },
//         );

//         ast.services.insert(
//             "BasicService".into(),
//             Service {
//                 name: "BasicService".into(),
//                 attributes: vec![ServiceAttribute {
//                     var_name: "db".into(),
//                     inject_reference: "D1Database".into(),
//                 }],
//                 initializer: None,
//                 methods,
//                 source_path: PathBuf::default(),
//             },
//         );
//     }

//     // plain old objects
//     {
//         ast.poos.insert(
//             "BasicPoo".into(),
//             PlainOldObject {
//                 name: "BasicPoo".into(),
//                 attributes: vec![
//                     Field {
//                         name: "field1".into(),
//                         cidl_type: CidlType::Text,
//                     },
//                     Field {
//                         name: "field2".into(),
//                         cidl_type: CidlType::Integer,
//                     },
//                 ],
//                 source_path: PathBuf::default(),
//             },
//         );

//         ast.poos.insert(
//             "PooWithComposition".into(),
//             PlainOldObject {
//                 name: "PooWithComposition".into(),
//                 attributes: vec![
//                     Field {
//                         name: "field1".into(),
//                         cidl_type: CidlType::Object("BasicPoo".into()),
//                     },
//                     Field {
//                         name: "field2".into(),
//                         cidl_type: CidlType::Object("BasicModel".into()),
//                     },
//                 ],
//                 source_path: PathBuf::default(),
//             },
//         );
//     }

//     let spec = create_spec(&ast);
//     SemanticAnalysis::analyze(&mut ast, &spec).expect("Semantic analysis to pass");
//     WorkersGenerator::generate_default_data_sources(&mut ast);
//     WorkersGenerator::finalize_api_methods(&mut ast);

//     let client_code = ClientGenerator::generate(&ast, "http://example.com/path/to/api");

//     insta::assert_snapshot!("client_code_generation_snapshot", client_code);
// }
