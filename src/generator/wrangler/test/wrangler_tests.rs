use std::collections::HashMap;

use ast::{CidlType, WranglerEnv};

use generator_test::{D1ModelBuilder, KVModelBuilder, create_ast, create_ast_d1, create_ast_kv};

use crate::{WranglerDefault, WranglerGenerator};

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
    let mut ast = create_ast(vec![], vec![]);
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
    let mut ast = create_ast_d1(vec![D1ModelBuilder::new("User").id().build()]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: Some("db".into()),
        vars: HashMap::new(),
        kv_bindings: vec![],
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
    let mut ast = create_ast_kv(vec![
        KVModelBuilder::new("MyKV", "MyKV", CidlType::JsonValue).build(),
    ]);
    ast.wrangler_env = Some(WranglerEnv {
        name: "Env".into(),
        source_path: "source.ts".into(),
        d1_binding: None,
        vars: HashMap::new(),
        kv_bindings: vec!["my_kv".into()],
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
