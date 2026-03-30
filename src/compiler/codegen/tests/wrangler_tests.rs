use codegen::wrangler::{WranglerDefault, WranglerGenerator};
use compiler_test::src_to_ast;

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
    let src = r#"
        env {
            db: d1
            API_KEY: string
            TIMEOUT: int
            ENABLED: bool
            THRESHOLD: double
        }
    "#;
    let ast = src_to_ast(src);

    // Act
    let specs = vec![
        {
            let mut spec = WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast, "migrations");
            spec
        },
        {
            let mut spec = WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast, "migrations");
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
    let src = r#"
        env {
            db: d1
        }

        @d1(db)
        model User {
            [primary id]
            id: int
        }
    "#;
    let ast = src_to_ast(src);

    // Act
    let specs = vec![
        {
            let mut spec = WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast, "my-migrations");
            spec
        },
        {
            let mut spec = WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast, "my-migrations");
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
        assert_eq!(
            spec.d1_databases[0].migrations_dir.as_ref().unwrap(),
            "my-migrations/db"
        );
    }
}

#[test]
fn generates_default_kv_wrangler_values() {
    // Arrange
    let src = r#"
        env {
            my_kv: kv
        }

        model MyKV {
            @kv(my_kv, "kvObj")
            obj: json
        }
    "#;
    let ast = src_to_ast(src);

    // Act
    let specs = vec![
        {
            let mut spec = WranglerGenerator::Toml(toml::from_str("").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast, "migrations");
            spec
        },
        {
            let mut spec = WranglerGenerator::Json(serde_json::from_str("{}").unwrap()).as_spec();
            WranglerDefault::set_defaults(&mut spec, &ast, "migrations");
            spec
        },
    ];

    // Assert
    for spec in specs {
        assert_eq!(spec.kv_namespaces.len(), 1);
        assert_eq!(spec.kv_namespaces[0].binding.as_ref().unwrap(), "my_kv");
        assert_eq!(
            spec.kv_namespaces[0].id.as_ref().unwrap(),
            "replace_with_my_kv_id"
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
    let ast = src_to_ast(
        r#"
            env {
                db: d1
            }
    
            @d1(db)
            model User {
                [primary id]
                id: int
            }
        "#,
    );

    // Act
    let mut spec =
        WranglerGenerator::Toml(toml::from_str(toml_with_incomplete_d1).unwrap()).as_spec();
    WranglerDefault::set_defaults(&mut spec, &ast, "default-migrations");

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
    assert_eq!(
        spec.d1_databases[0].migrations_dir.as_ref().unwrap(),
        "default-migrations/db"
    );

    let mut generator = WranglerGenerator::Toml(toml::from_str(toml_with_incomplete_d1).unwrap());
    let result = generator.generate(spec);
    assert!(result.contains("replace_with_db_id"));
}
