use std::collections::HashMap;

use ast::{CidlType, WranglerEnv};

use generator_test::{ModelBuilder, create_ast};
use wrangler::{WranglerDefault, WranglerGenerator};

#[test]
fn test_serialize_wrangler_spec() {
    // Empty TOML
    {
        WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec();
    }

    // Empty JSON
    {
        WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec();
    }
}

#[test]
fn generates_default_wrangler_value() {
    // Arrange
    let mut ast = create_ast(vec![]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: Some("db".into()),
        vars: [
            ("API_KEY".into(), CidlType::Text),
            ("TIMEOUT".into(), CidlType::Integer),
            ("ENABLED".into(), CidlType::Boolean),
            ("THRESHOLD".into(), CidlType::Real),
        ]
        .into_iter()
        .collect(),
        kv_bindings: vec![],
        r2_bindings: vec![],
    });

    // Act
    let specs = vec![
        {
            let mut spec = WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast);
            spec
        },
        {
            let mut spec = WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast);
            spec
        },
    ];

    // Assert
    for spec in specs {
        assert_eq!(spec.name.unwrap(), "cloesce");
        assert_eq!(spec.compatibility_date.unwrap(), "2025-10-02");
        assert_eq!(spec.main.unwrap(), "workers.ts");
        assert_eq!(spec.vars.get("API_KEY").unwrap(), "default_string");
        assert_eq!(spec.vars.get("TIMEOUT").unwrap(), "0");
        assert_eq!(*spec.vars.get("ENABLED").unwrap(), "false");
        assert_eq!(*spec.vars.get("THRESHOLD").unwrap(), "0");
    }
}

#[test]
fn generates_default_d1_wrangler_values() {
    // Arrange
    let mut ast = create_ast(vec![ModelBuilder::new("User").id_pk().build()]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: Some("db".into()),
        vars: HashMap::new(),
        kv_bindings: vec![],
        r2_bindings: vec![],
    });

    // Act
    let specs = vec![
        {
            let mut spec = WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast);
            spec
        },
        {
            let mut spec = WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast);
            spec
        },
    ];

    // Assert
    for spec in specs {
        assert_eq!(spec.d1_databases.len(), 1);
        assert_eq!(spec.d1_databases[0].binding.as_ref().unwrap(), "db");
        assert_eq!(
            spec.d1_databases[0].database_name.as_ref().unwrap(),
            "replace_with_db_name"
        );
        assert_eq!(
            spec.d1_databases[0].database_id.as_ref().unwrap(),
            "replace_with_db_id"
        );
    }
}

#[test]
fn generates_default_kv_wrangler_values() {
    // Arrange
    let mut ast = create_ast(vec![
        // ModelBuilder::new("MyKV", "MyKV", CidlType::JsonValue).build(),
        ModelBuilder::new("MyKV")
            .kv_object("obj", "my_kv", "kvObj", false, CidlType::JsonValue)
            .build(),
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: None,
        vars: HashMap::new(),
        kv_bindings: vec!["my_kv".into()],
        r2_bindings: vec![],
    });

    // Act
    let specs = vec![
        {
            let mut spec = WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast);
            spec
        },
        {
            let mut spec = WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast);
            spec
        },
    ];

    // Assert
    for spec in specs {
        assert_eq!(spec.kv_namespaces.len(), 1);
        assert_eq!(spec.kv_namespaces[0].binding.as_ref().unwrap(), "my_kv");
        assert_eq!(
            spec.kv_namespaces[0].id.as_ref().unwrap(),
            "replace_with_kv_id"
        );
    }
}

#[test]
fn handles_d1_database_with_missing_values() {
    // Arrange
    let toml_with_incomplete_d1 = r#"
        [[d1_databases]]
        binding = "db"
    "#;

    let mut ast = create_ast(vec![ModelBuilder::new("User").id_pk().build()]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: Some("db".into()),
        vars: HashMap::new(),
        kv_bindings: vec![],
        r2_bindings: vec![],
    });

    // Act
    let mut spec =
        WranglerGenerator::Toml(toml::from_str(toml_with_incomplete_d1).unwrap()).as_spec();
    WranglerDefault::set_defaults(&mut spec, &ast);

    // Assert
    assert_eq!(spec.d1_databases.len(), 1);
    assert_eq!(spec.d1_databases[0].binding.as_ref().unwrap(), "db");
    assert_eq!(
        spec.d1_databases[0].database_name.as_ref().unwrap(),
        "replace_with_db_name"
    );
    assert_eq!(
        spec.d1_databases[0].database_id.as_ref().unwrap(),
        "replace_with_db_id"
    );

    let temp_dir = std::env::temp_dir();
    let test_file_path = temp_dir.join("test_wrangler.toml");
    let wrangler_file = std::fs::File::create(&test_file_path).unwrap();

    let mut generator = WranglerGenerator::Toml(toml::from_str(toml_with_incomplete_d1).unwrap());

    generator.generate(spec, wrangler_file);

    let generated_content = std::fs::read_to_string(&test_file_path).unwrap();
    assert!(generated_content.contains("replace_with_db_id"));
    assert!(generated_content.contains("replace_with_db_name"));

    // Cleanup
    std::fs::remove_file(test_file_path).ok();
}
