use std::path::Path;

use ast::{CidlType, CrudKind, HttpVerb, MediaType, NamedTypedValue};
use generator_test::{ModelBuilder, create_ast};
use workers::WorkersGenerator;

#[test]
fn link_generates_relative_import_for_model() {
    // Arrange
    let workers_path = Path::new("/project/workers/index.ts");

    let mut user = ModelBuilder::new("User").id_pk().build();
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

    let mut thing = ModelBuilder::new("Thing").id_pk().build();
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

    let mut alien = ModelBuilder::new("Alien").id_pk().build();
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
    let mut user = ModelBuilder::new("User").id_pk().build();
    user.cruds
        .extend(vec![CrudKind::GET, CrudKind::SAVE, CrudKind::LIST]);

    let mut ast = create_ast(vec![user]);

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast);

    // Assert
    let user = ast.models.get("User").unwrap();

    assert!(user.methods.contains_key("GET"));
    assert!(user.methods.contains_key("LIST"));
    assert!(user.methods.contains_key("SAVE"));
}

#[test]
fn finalize_does_not_overwrite_existing_method() {
    // Arrange
    let mut user = ModelBuilder::new("User")
        .id_pk()
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

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast);

    // Assert
    let user = ast.models.get("User").unwrap();
    let method = user.methods.get("get").unwrap();

    assert_eq!(method.http_verb, HttpVerb::POST);
    assert_eq!(method.parameters.len(), 1);
}

#[test]
fn finalize_sets_json_media_type() {
    // Arrange
    let mut user = ModelBuilder::new("User").id_pk().build();
    user.cruds.extend(vec![CrudKind::GET]);

    let mut ast = create_ast(vec![user]);

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast);

    // Assert
    let mut user = ast.models.shift_remove("User").unwrap();
    let (_, method) = user.methods.pop_first().unwrap();
    assert!(matches!(method.return_media, MediaType::Json));
    assert!(matches!(method.parameters_media, MediaType::Json));
}

#[test]
fn finalize_sets_octet_media_type() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("User")
            .id_pk()
            .method(
                "acceptReturnOctet",
                HttpVerb::POST,
                true,
                vec![NamedTypedValue {
                    name: "stream".into(),
                    cidl_type: CidlType::Stream,
                }],
                CidlType::Stream,
            )
            .build(),
    ]);

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast);

    // Assert
    let mut user = ast.models.shift_remove("User").unwrap();
    let method = user.methods.remove("acceptReturnOctet").unwrap();
    assert!(matches!(method.return_media, MediaType::Octet));
    assert!(matches!(method.parameters_media, MediaType::Octet));
}

#[test]
fn finalize_adds_datasource_parameter_to_instance_method() {
    // Arrange
    let mut ast = create_ast(vec![
        ModelBuilder::new("User")
            .id_pk()
            // No datasource parameter initially
            .method(
                "instanceMethod",
                HttpVerb::GET,
                false,
                vec![],
                CidlType::Object("User".into()),
            )
            // Static, does not need datasource parameter
            .method(
                "staticMethod",
                HttpVerb::GET,
                true,
                vec![],
                CidlType::Object("User".into()),
            )
            // Has a datasource parameter already
            .method(
                "methodWithDatasource",
                HttpVerb::GET,
                false,
                vec![NamedTypedValue {
                    name: "datasource".into(),
                    cidl_type: CidlType::DataSource("User".into()),
                }],
                CidlType::Object("User".into()),
            )
            .build(),
    ]);

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast);

    // Assert
    let mut user = ast.models.shift_remove("User").unwrap();
    let instance_method = user.methods.remove("instanceMethod").unwrap();
    let static_method = user.methods.remove("staticMethod").unwrap();
    let method_with_datasource = user.methods.remove("methodWithDatasource").unwrap();

    assert!(
        instance_method
            .parameters
            .iter()
            .any(|p| matches!(p.cidl_type, CidlType::DataSource(_))),
        "Instance method should have a __datasource parameter"
    );

    assert!(
        !static_method
            .parameters
            .iter()
            .any(|p| matches!(p.cidl_type, CidlType::DataSource(_))),
        "Static method should not have a __datasource parameter"
    );

    assert_eq!(
        method_with_datasource
            .parameters
            .iter()
            .filter(|p| matches!(p.cidl_type, CidlType::DataSource(_)))
            .count(),
        1,
        "Method should not have duplicate __datasource parameters"
    );
}

#[test]
fn finalize_get_crud_adds_primary_key_for_d1_model() {
    // Arrange
    let mut user = ModelBuilder::new("User").id_pk().build();
    user.cruds.push(CrudKind::GET);

    let mut ast = create_ast(vec![user]);

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast);

    // Assert
    let user = ast.models.get("User").unwrap();
    let get_method = user.methods.get("GET").unwrap();

    // Should have datasource parameter
    assert!(
        get_method
            .parameters
            .iter()
            .any(|p| matches!(p.cidl_type, CidlType::DataSource(_))),
        "GET method should have __datasource parameter"
    );

    // Should have primary key parameter (id) when model has D1
    assert!(
        get_method.parameters.iter().any(|p| p.name == "id"),
        "GET method should have primary key parameter for D1 model"
    );

    assert_eq!(get_method.http_verb, HttpVerb::GET);
    assert!(get_method.is_static);
}

#[test]
fn finalize_get_crud_adds_key_params() {
    // Arrange
    let mut product = ModelBuilder::new("Product")
        .id_pk()
        .key_param("category")
        .key_param("subcategory")
        .build();
    product.cruds.push(CrudKind::GET);

    let mut ast = create_ast(vec![product]);

    // Act
    WorkersGenerator::finalize_api_methods(&mut ast);

    // Assert
    let product = ast.models.get("Product").unwrap();
    let get_method = product.methods.get("GET").unwrap();

    // Should have datasource parameter
    assert!(
        get_method
            .parameters
            .iter()
            .any(|p| matches!(p.cidl_type, CidlType::DataSource(_))),
        "GET method should have __datasource parameter"
    );

    // Should have key_params as Text type
    let category_param = get_method.parameters.iter().find(|p| p.name == "category");
    assert!(category_param.is_some(), "Should have category key param");
    assert!(
        matches!(category_param.unwrap().cidl_type, CidlType::Text),
        "Key params should be Text type"
    );

    let subcategory_param = get_method
        .parameters
        .iter()
        .find(|p| p.name == "subcategory");
    assert!(
        subcategory_param.is_some(),
        "Should have subcategory key param"
    );
    assert!(
        matches!(subcategory_param.unwrap().cidl_type, CidlType::Text),
        "Key params should be Text type"
    );
}
