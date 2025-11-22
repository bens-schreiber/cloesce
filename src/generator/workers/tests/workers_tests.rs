use std::path::Path;

use ast::{
    CidlType, CrudKind, HttpVerb, MediaType, NamedTypedValue,
    builder::{ModelBuilder, create_ast},
    semantic::SemanticAnalysis,
};
use workers::WorkersGenerator;

#[test]
fn link_generates_relative_import_for_model() {
    // Arrange
    let workers_path = Path::new("/project/workers/index.ts");

    let mut user = ModelBuilder::new("User").id().build();
    user.source_path = Path::new("/project/models/User.ts").to_path_buf();

    let ast = create_ast(vec![user]);

    // Act
    let result = WorkersGenerator::link(&ast, workers_path);

    assert!(
        result.contains(r#"import { User } from "../models/User""#),
        "Expected relative path to models folder, got:\n{result}"
    );
}

#[test]
fn link_adds_dot_slash_when_model_in_same_directory() {
    // Arrange
    let workers_path = Path::new("/project/workers/index.ts");

    let mut thing = ModelBuilder::new("Thing").id().build();
    thing.source_path = Path::new("/project/workers/Thing.ts").to_path_buf();

    let ast = create_ast(vec![thing]);

    // Act
    let result = WorkersGenerator::link(&ast, workers_path);

    // Assert
    assert!(
        result.contains(r#"import { Thing } from "./Thing""#),
        "Expected './Thing' import when model is in same folder:\n{result}"
    );
}

#[test]
fn link_falls_back_to_absolute_path_when_relative_not_possible() {
    // Arrange
    let workers_path = Path::new("/project/workers/index.ts");

    let mut alien = ModelBuilder::new("Alien").id().build();
    alien.source_path = Path::new("C:\\nonrelative\\Alien.ts").to_path_buf();

    let ast = create_ast(vec![alien]);

    // Act
    let result = WorkersGenerator::link(&ast, workers_path);

    // Assert
    assert!(
        result.contains(r#"import { Alien } from "C:\nonrelative\Alien.ts""#),
        "Expected fallback to absolute path when relative calc fails:\n{result}"
    );
}

#[test]
fn finalize_adds_crud_methods_to_model() {
    // Arrange
    let mut user = ModelBuilder::new("User").id().build();
    user.cruds
        .extend(vec![CrudKind::GET, CrudKind::SAVE, CrudKind::LIST]);

    let mut ast = create_ast(vec![user]);
    let blob_objects = SemanticAnalysis::analyze(&mut ast).expect("analysis ok");

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast, &blob_objects);

    // Assert
    let user = ast.models.get("User").unwrap();

    assert!(user.methods.contains_key("get"));
    assert!(user.methods.contains_key("list"));
    assert!(user.methods.contains_key("save"));
}

#[test]
fn finalize_does_not_overwrite_existing_method() {
    // Arrange
    let mut user = ModelBuilder::new("User")
        .id()
        .method(
            "get",
            HttpVerb::POST,
            true,
            vec![NamedTypedValue {
                name: "id".into(),
                cidl_type: CidlType::Integer,
            }],
            CidlType::Void,
        )
        .build();
    user.cruds.push(CrudKind::GET);

    let mut ast = create_ast(vec![user]);
    let blob_objects = SemanticAnalysis::analyze(&mut ast).expect("analysis ok");

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast, &blob_objects);

    // Assert
    let user = ast.models.get("User").unwrap();
    let method = user.methods.get("get").unwrap();

    assert_eq!(method.http_verb, HttpVerb::POST);
    assert_eq!(method.parameters.len(), 1);
}

#[test]
fn finalize_sets_json_media_type() {
    // Arrange
    let mut user = ModelBuilder::new("User").id().build();
    user.cruds.extend(vec![CrudKind::GET]);

    let mut ast = create_ast(vec![user]);
    let blob_objects = SemanticAnalysis::analyze(&mut ast).expect("analysis ok");

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast, &blob_objects);

    // Assert
    let mut user = ast.models.shift_remove("User").unwrap();
    let (_, method) = user.methods.pop_first().unwrap();
    assert!(matches!(method.return_media, MediaType::Json));
    assert!(matches!(method.parameters_media, MediaType::Json));
}

#[test]
fn finalize_sets_formdata_media_type() {
    // Arrange
    let mut user = ModelBuilder::new("User")
        .id()
        .attribute("blob", CidlType::Blob, None)
        .build();
    user.cruds.extend(vec![CrudKind::GET, CrudKind::SAVE]);

    let mut ast = create_ast(vec![user]);
    let blob_objects = SemanticAnalysis::analyze(&mut ast).expect("analysis ok");

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast, &blob_objects);

    // Assert
    let mut user = ast.models.shift_remove("User").unwrap();
    let get = user.methods.remove("get").unwrap();
    assert!(matches!(get.return_media, MediaType::FormData));
    assert!(matches!(get.parameters_media, MediaType::Json));

    let save = user.methods.remove("save").unwrap();
    assert!(matches!(save.return_media, MediaType::FormData));
    assert!(matches!(save.parameters_media, MediaType::FormData));
}

#[test]
fn finalize_sets_octet_media_type() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("User")
            .id()
            .method(
                "acceptReturnOctet",
                HttpVerb::POST,
                true,
                vec![NamedTypedValue {
                    name: "blob".into(),
                    cidl_type: CidlType::Blob,
                }],
                CidlType::Blob,
            )
            .build(),
    ]);
    let blob_objects = SemanticAnalysis::analyze(&mut ast).expect("analysis ok");

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast, &blob_objects);

    // Assert
    let mut user = ast.models.shift_remove("User").unwrap();
    let method = user.methods.remove("acceptReturnOctet").unwrap();
    assert!(matches!(method.return_media, MediaType::Octet));
    assert!(matches!(method.parameters_media, MediaType::Octet));
}
