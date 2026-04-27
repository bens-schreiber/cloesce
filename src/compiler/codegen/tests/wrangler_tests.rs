use codegen::wrangler::{WranglerDefault, WranglerGenerator};
use compiler_test::src_to_ast;

#[test]
fn test_serialize_wrangler_spec() {
    // Empty TOML
    {
        WranglerGenerator::Toml(toml::from_str("").unwrap())
            .as_spec(None)
            .expect("Empty TOML should produce a valid spec");
    }

    // Empty JSON
    {
        WranglerGenerator::Json(serde_json::from_str("{}").unwrap())
            .as_spec(None)
            .expect("Empty JSON should produce a valid spec");
    }
}

#[test]
fn generates_default_wrangler_value() {
    // Arrange
    let src = r#"
        env {
            d1 { db }
            vars {
                API_KEY: string
                TIMEOUT: int
                ENABLED: bool
                THRESHOLD: real
            }
        }
    "#;
    let ast = src_to_ast(src);

    // Act
    let specs = vec![
        {
            let mut spec = WranglerGenerator::Toml(toml::from_str("").unwrap())
                .as_spec(None)
                .unwrap();
            WranglerDefault::set_defaults(&mut spec, &ast, "migrations");
            spec
        },
        {
            let mut spec = WranglerGenerator::Json(serde_json::from_str("{}").unwrap())
                .as_spec(None)
                .unwrap();
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
            d1 { db }
        }

        [use db]
        model User {
            primary {
                id: int
            }
        }
    "#;
    let ast = src_to_ast(src);

    // Act
    let specs = vec![
        {
            let mut spec = WranglerGenerator::Toml(toml::from_str("").unwrap())
                .as_spec(None)
                .unwrap();
            WranglerDefault::set_defaults(&mut spec, &ast, "my-migrations");
            spec
        },
        {
            let mut spec = WranglerGenerator::Json(serde_json::from_str("{}").unwrap())
                .as_spec(None)
                .unwrap();
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
            kv { my_kv }
        }

        model MyKV {
            kv(my_kv, "kvObj") {
                obj: json
            }
        }
    "#;
    let ast = src_to_ast(src);

    // Act
    let specs = vec![
        {
            let mut spec = WranglerGenerator::Toml(toml::from_str("").unwrap())
                .as_spec(None)
                .unwrap();
            WranglerDefault::set_defaults(&mut spec, &ast, "migrations");
            spec
        },
        {
            let mut spec = WranglerGenerator::Json(serde_json::from_str("{}").unwrap())
                .as_spec(None)
                .unwrap();
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
                d1 { db }
            }

            [use db]
            model User {
                primary {
                    id: int
                }
            }
        "#,
    );

    // Act
    let mut spec = WranglerGenerator::Toml(toml::from_str(toml_with_incomplete_d1).unwrap())
        .as_spec(None)
        .unwrap();
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
    let result = generator.generate(spec, None);
    assert!(result.contains("replace_with_db_id"));
}

#[test]
fn env_reads_from_env_block_and_falls_back_to_root() {
    // TOML: env block overrides the top-level d1 binding id; name falls back to root
    let toml_src = r#"
        name = "my-worker"
        compatibility_date = "2025-01-01"
        main = "index.ts"

        [[d1_databases]]
        binding = "DB"
        database_id = "prod-id"
        database_name = "prod-db"
        migrations_dir = "migrations/DB"

        [[env.staging.d1_databases]]
        binding = "DB"
        database_id = "staging-id"
        database_name = "staging-db"
        migrations_dir = "migrations/DB"
    "#;

    let toml_spec = WranglerGenerator::Toml(toml::from_str(toml_src).unwrap())
        .as_spec(Some("staging"))
        .unwrap();
    assert_eq!(
        toml_spec.name.as_deref(),
        Some("my-worker"),
        "name should fall back to root"
    );
    assert_eq!(toml_spec.d1_databases.len(), 1);
    assert_eq!(
        toml_spec.d1_databases[0].database_id.as_deref(),
        Some("staging-id")
    );

    let toml_root_spec = WranglerGenerator::Toml(toml::from_str(toml_src).unwrap())
        .as_spec(None)
        .unwrap();
    assert_eq!(
        toml_root_spec.d1_databases[0].database_id.as_deref(),
        Some("prod-id")
    );

    let json_src = r#"{
        "name": "my-worker",
        "compatibility_date": "2025-01-01",
        "main": "index.ts",
        "d1_databases": [
            { "binding": "DB", "database_id": "prod-id", "database_name": "prod-db", "migrations_dir": "migrations/DB" }
        ],
        "env": {
            "staging": {
                "d1_databases": [
                    { "binding": "DB", "database_id": "staging-id", "database_name": "staging-db", "migrations_dir": "migrations/DB" }
                ]
            }
        }
    }"#;

    let json_spec = WranglerGenerator::Json(serde_json::from_str(json_src).unwrap())
        .as_spec(Some("staging"))
        .unwrap();
    assert_eq!(
        json_spec.name.as_deref(),
        Some("my-worker"),
        "name should fall back to root"
    );
    assert_eq!(json_spec.d1_databases.len(), 1);
    assert_eq!(
        json_spec.d1_databases[0].database_id.as_deref(),
        Some("staging-id")
    );

    // Verify top-level spec is unaffected
    let json_root_spec = WranglerGenerator::Json(serde_json::from_str(json_src).unwrap())
        .as_spec(None)
        .unwrap();
    assert_eq!(
        json_root_spec.d1_databases[0].database_id.as_deref(),
        Some("prod-id")
    );
}

#[test]
fn env_generate_writes_into_env_block() {
    let ast = src_to_ast(
        r#"
        env {
            d1 { DB }
            kv { CACHE }
        }

        [use DB]
        model User {
            primary { id: int }
        }
    "#,
    );

    // TOML: should write under env.staging
    {
        let toml_src = r#"
            name = "my-worker"

            [[d1_databases]]
            binding = "DB"
            database_id = "prod-id"
            database_name = "prod-db"
            migrations_dir = "migrations/DB"

            [[kv_namespaces]]
            binding = "CACHE"
            id = "prod-cache-id"
        "#;

        let mut generator = WranglerGenerator::Toml(toml::from_str(toml_src).unwrap());
        let mut spec = generator.as_spec(Some("staging")).unwrap();
        WranglerDefault::set_defaults(&mut spec, &ast, "migrations");
        let output = generator.generate(spec, Some("staging"));

        assert!(
            output.contains("[env.staging."),
            "expected env.staging section in TOML output:\n{output}"
        );

        let root_spec = WranglerGenerator::Toml(toml::from_str(&output).unwrap())
            .as_spec(None)
            .unwrap();
        assert_eq!(
            root_spec.d1_databases[0].database_id.as_deref(),
            Some("prod-id")
        );
        assert_eq!(
            root_spec.kv_namespaces[0].id.as_deref(),
            Some("prod-cache-id")
        );

        // Staging env reads back the generated values
        let staging_spec = WranglerGenerator::Toml(toml::from_str(&output).unwrap())
            .as_spec(Some("staging"))
            .unwrap();
        assert!(staging_spec.d1_databases[0].database_id.is_some());
        assert!(staging_spec.kv_namespaces[0].id.is_some());
    }

    // JSON: should write under env.staging
    {
        let json_src = r#"{
            "name": "my-worker",
            "d1_databases": [
                { "binding": "DB", "database_id": "prod-id", "database_name": "prod-db", "migrations_dir": "migrations/DB" }
            ],
            "kv_namespaces": [
                { "binding": "CACHE", "id": "prod-cache-id" }
            ]
        }"#;

        let mut generator = WranglerGenerator::Json(serde_json::from_str(json_src).unwrap());
        let mut spec = generator.as_spec(Some("staging")).unwrap();
        WranglerDefault::set_defaults(&mut spec, &ast, "migrations");
        let output = generator.generate(spec, Some("staging"));

        let output_val: serde_json::Value = serde_json::from_str(&output).unwrap();

        assert!(
            output_val["env"]["staging"]["d1_databases"].is_array(),
            "expected env.staging.d1_databases in JSON output"
        );

        assert_eq!(output_val["d1_databases"][0]["database_id"], "prod-id");
        assert_eq!(output_val["kv_namespaces"][0]["id"], "prod-cache-id");

        let staging_spec = WranglerGenerator::Json(serde_json::from_str(&output).unwrap())
            .as_spec(Some("staging"))
            .unwrap();
        assert!(staging_spec.d1_databases[0].database_id.is_some());
        assert!(staging_spec.kv_namespaces[0].id.is_some());
    }
}
