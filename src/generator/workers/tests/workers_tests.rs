use std::path::Path;

use ast::{CidlType, CrudKind, HttpVerb, MediaType, NamedTypedValue, NavigationPropertyKind};
use generator_test::{ModelBuilder, create_ast};
use workers::WorkersGenerator;

#[test]
fn link_generates_correct_imports() {
    // Generates relative import for model
    {
        // Arrange
        let workers_path = Path::new("/project/workers/index.ts");
        let mut user = ModelBuilder::new("User").id_pk().build();
        user.source_path = Path::new("/project/models/User.ts").to_path_buf();
        let ast = create_ast(vec![user]);

        // Act
        let result = WorkersGenerator::link(&ast, workers_path);

        // Assert
        assert!(
            result.contains(r#"import { User } from "../models/User.js""#),
            "got: \n{result}"
        );
    }

    // Adds dot slash when model in same directory
    {
        // Arrange
        let workers_path = Path::new("/project/workers/index.ts");
        let mut thing = ModelBuilder::new("Thing").id_pk().build();
        thing.source_path = Path::new("/project/workers/Thing.ts").to_path_buf();
        let ast = create_ast(vec![thing]);

        // Act
        let result = WorkersGenerator::link(&ast, workers_path);

        // Assert
        assert!(
            result.contains(r#"import { Thing } from "./Thing.js""#),
            "got: \n{result}"
        );
    }

    // Falls back to absolute path when relative not possible
    {
        // Arrange
        let workers_path = Path::new("/project/workers/index.ts");
        let mut alien = ModelBuilder::new("Alien").id_pk().build();
        alien.source_path = Path::new("C:\\nonrelative\\Alien.ts").to_path_buf();
        let ast = create_ast(vec![alien]);

        // Act
        let result = WorkersGenerator::link(&ast, workers_path);

        // Assert
        assert!(
            result.contains(r#"import { Alien } from "C:/nonrelative/Alien.js""#),
            "got: \n{result}"
        );
    }

    // Handles nested paths with forward slashes
    {
        // Arrange
        let workers_path = Path::new("project/src/workers/index.ts");
        let mut model = ModelBuilder::new("Model").id_pk().build();
        model.source_path = Path::new("project/src/models/deep/Model.ts").to_path_buf();
        let ast = create_ast(vec![model]);

        // Act
        let result = WorkersGenerator::link(&ast, workers_path);

        // Assert
        assert!(
            result.contains(r#"import { Model } from "../models/deep/Model.js""#),
            "got:\n{result}"
        );
    }

    // Windows absolute path with forward slashes
    {
        // Arrange
        let workers_path =
            Path::new("C:/Users/vmtest/Desktop/cloescetest/my-cloesce-app/.generated/workers.ts");
        let mut model = ModelBuilder::new("Model").id_pk().build();
        model.source_path = Path::new(
            "C:/Users/vmtest/Desktop/cloescetest/my-cloesce-app/src/data/models.cloesce.ts",
        )
        .to_path_buf();
        let ast = create_ast(vec![model]);

        // Act
        let result = WorkersGenerator::link(&ast, workers_path);

        // Assert
        assert!(
            result.contains(r#"import { Model } from "../src/data/models.cloesce.js""#),
            "got:\n{result}"
        );
    }
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
            None,
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
                None,
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

#[test]
fn generate_default_data_sources() {
    // Arrange
    let profile = ModelBuilder::new("Profile").id_pk().build();

    let role = ModelBuilder::new("Role").id_pk().build();

    let order = ModelBuilder::new("Order").id_pk().build();

    let user = ModelBuilder::new("User")
        .id_pk()
        // 1:1 relationship to Profile
        .nav_p(
            "profile",
            "Profile",
            NavigationPropertyKind::OneToOne {
                column_reference: "profileId".into(),
            },
        )
        // 1:M relationship to Order
        .nav_p(
            "orders",
            "Order",
            NavigationPropertyKind::OneToMany {
                column_reference: "userId".into(),
            },
        )
        // M:M relationship to Role
        .nav_p("roles", "Role", NavigationPropertyKind::ManyToMany)
        // KV object for caching
        .kv_object(
            "json",
            "kv_namespace",
            "userCache",
            false,
            CidlType::Object("User".into()),
        )
        // R2 object for storage
        .r2_object("binary", "r2_namespace", "userDocuments", false)
        .build();

    let mut ast = create_ast(vec![profile, role, order, user]);

    // Act
    WorkersGenerator::generate_default_data_sources(&mut ast);

    // Assert
    let user = ast.models.get("User").unwrap();
    let default_ds = user
        .default_data_source()
        .expect("User should have default data source");
    let tree = &default_ds.tree;

    assert!(
        tree.0.contains_key("profile"),
        "Default data source should include 1:1 relationship 'profile'"
    );

    assert!(
        tree.0.contains_key("orders"),
        "Default data source should include 1:M relationship 'orders'"
    );

    assert!(
        tree.0.contains_key("roles"),
        "Default data source should include M:M relationship 'roles'"
    );

    assert!(
        tree.0.contains_key("userCache"),
        "Default data source should include KV object 'userCache'"
    );

    assert!(
        tree.0.contains_key("userDocuments"),
        "Default data source should include R2 object 'userDocuments'"
    );

    assert!(
        !default_ds.is_private,
        "Default data source should be public"
    );

    assert_eq!(
        default_ds.name, "default",
        "Data source should be named 'default'"
    );
}

#[test]
fn generate_default_data_sources_does_not_include_manys() {
    // Arrange
    let grade = ModelBuilder::new("Grade").id_pk().build();

    let teacher = ModelBuilder::new("Teacher")
        .id_pk()
        .nav_p(
            "students",
            "Student",
            NavigationPropertyKind::OneToMany {
                column_reference: "teacherId".into(),
            },
        )
        .build();

    let student = ModelBuilder::new("Student")
        .id_pk()
        .nav_p("teachers", "Teacher", NavigationPropertyKind::ManyToMany)
        .nav_p(
            "grades",
            "Grade",
            NavigationPropertyKind::OneToMany {
                column_reference: "studentId".into(),
            },
        )
        .build();

    let mut ast = create_ast(vec![grade, teacher, student]);

    // Act
    WorkersGenerator::generate_default_data_sources(&mut ast);

    // Assert
    let teacher = ast.models.get("Teacher").unwrap();
    let default_ds = teacher
        .default_data_source()
        .expect("Teacher should have default data source");
    let tree = &default_ds.tree;

    assert!(
        tree.0.contains_key("students"),
        "Default data source for Teacher should include 'students' relationship"
    );

    let students_node = tree.0.get("students").unwrap();
    assert!(
        !students_node.0.contains_key("grades"),
        "Default data source for Teacher should NOT include nested 'grades' under 'students'"
    );
}

#[test]
fn generate_default_data_sources_includes_multiple_one_to_ones() {
    // Arrange
    let toy = ModelBuilder::new("Toy").id_pk().build();
    // dog has a toy
    let dog = ModelBuilder::new("Dog")
        .id_pk()
        .nav_p(
            "toy",
            "Toy",
            NavigationPropertyKind::OneToOne {
                column_reference: "toyId".into(),
            },
        )
        .build();

    // owner has a dog
    let owner = ModelBuilder::new("Owner")
        .id_pk()
        .nav_p(
            "dog",
            "Dog",
            NavigationPropertyKind::OneToOne {
                column_reference: "dogId".into(),
            },
        )
        .build();

    let mut ast = create_ast(vec![toy, dog, owner]);

    // Act
    WorkersGenerator::generate_default_data_sources(&mut ast);

    // Assert
    let owner = ast.models.get("Owner").unwrap();
    let default_ds = owner
        .default_data_source()
        .expect("Owner should have default data source");
    let tree = &default_ds.tree;
    assert!(
        tree.0.contains_key("dog"),
        "Default data source for Owner should include 'dog' relationship"
    );

    let dog_node = tree.0.get("dog").unwrap();
    assert!(
        dog_node.0.contains_key("toy"),
        "Default data source for Owner should include 'toy' relationship under 'dog'"
    );

    let toy_node = dog_node.0.get("toy").unwrap();
    assert!(
        toy_node.0.is_empty(),
        "Default data source for Owner should NOT include any nested relationships under 'toy'"
    );
}
