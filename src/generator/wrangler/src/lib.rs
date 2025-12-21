use std::collections::HashMap;
use std::path::Path;
use std::{fs::File, io::Write};

use ast::err::GeneratorErrorKind;
use ast::{CidlType, CloesceAst, ensure, err::Result, fail};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct D1Database {
    pub binding: Option<String>,
    pub database_name: Option<String>,
    pub database_id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct KVNamespace {
    pub binding: Option<String>,
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct WranglerSpec {
    pub name: Option<String>,
    pub compatibility_date: Option<String>,
    pub main: Option<String>,

    #[serde(default)]
    pub d1_databases: Vec<D1Database>,

    #[serde(default)]
    pub kv_namespaces: Vec<KVNamespace>,

    #[serde(default)]
    pub vars: HashMap<String, String>,
}

impl WranglerSpec {
    /// Ensures that all required values exist or places a default
    /// for them
    pub fn generate_defaults(&mut self, ast: &CloesceAst) {
        // Generate default worker entry point values
        self.name = Some(self.name.clone().unwrap_or_else(|| {
            tracing::warn!("Set a default worker name \"cloesce\"");
            "cloesce".to_string()
        }));

        self.compatibility_date = Some(self.compatibility_date.clone().unwrap_or_else(|| {
            tracing::warn!("Set a default compatibility date.");
            "2025-10-02".to_string()
        }));

        self.main = Some(
            self.main
                .clone()
                .unwrap_or_else(|| "workers.ts".to_string()),
        );

        // Ensure all bindings referenced in the WranglerEnv exist in the spec
        if let Some(env) = &ast.wrangler_env {
            if let Some(db_binding) = &env.d1_binding {
                let db = self
                    .d1_databases
                    .iter_mut()
                    .find(|db| db.binding.as_deref() == Some(db_binding));

                match db {
                    Some(db) => {
                        if db.database_id.is_none() {
                            db.database_id = Some("replace_with_db_id".into());
                            tracing::warn!(
                                "D1 Database with binding {} is missing an id. See https://developers.cloudflare.com/d1/get-started/",
                                db_binding
                            );
                        }

                        if db.database_name.is_none() {
                            db.database_name = Some("replace_with_db_name".into());
                            tracing::warn!(
                                "D1 Database with binding {} is missing a name. See https://developers.cloudflare.com/d1/get-started/",
                                db_binding
                            );
                        }
                    }
                    None => {
                        self.d1_databases.push(D1Database {
                            binding: Some(db_binding.clone()),
                            database_name: Some("replace_with_db_name".into()),
                            database_id: Some("replace_with_db_id".into()),
                        });

                        tracing::warn!(
                            "D1 Database with binding {} was missing, added a default. See https://developers.cloudflare.com/d1/get-started/",
                            db_binding
                        );
                    }
                }
            }

            for kv_binding in &env.kv_bindings {
                let kv = self
                    .kv_namespaces
                    .iter_mut()
                    .find(|ns| ns.binding.as_deref() == Some(kv_binding));

                match kv {
                    Some(ns) => {
                        if ns.id.is_none() {
                            ns.id = Some("replace_with_kv_id".into());
                            tracing::warn!(
                                "KV Namespace with binding {} is missing an id. See https://developers.cloudflare.com/workers/platform/storage/#namespaces",
                                kv_binding
                            );
                        }
                    }
                    None => {
                        self.kv_namespaces.push(KVNamespace {
                            binding: Some(kv_binding.clone()),
                            id: Some("replace_with_kv_id".into()),
                        });

                        tracing::warn!(
                            "KV Namespace with binding {} was missing, added a default. See https://developers.cloudflare.com/workers/platform/storage/#namespaces",
                            kv_binding
                        );
                    }
                }
            }
        }

        // Generate default vars from the AST's WranglerEnv
        if let Some(env) = &ast.wrangler_env {
            for (var, ty) in &env.vars {
                self.vars.entry(var.clone()).or_insert_with(|| {
                    let default = match ty {
                        CidlType::Text => "default_string",
                        CidlType::Integer | CidlType::Real => "0",
                        CidlType::Boolean => "false",
                        _ => "default_value",
                    };
                    tracing::warn!("Added missing Wrangler var {var} with a default value");
                    default.into()
                });
            }
        }
    }

    pub fn validate_ast_matches_wrangler(&self, ast: &CloesceAst) -> Result<()> {
        let env = match &ast.wrangler_env {
            Some(env) => env,

            // No models are defined, no env is required
            None if ast.d1_models.is_empty() && ast.kv_models.is_empty() => return Ok(()),

            // Models are defined but an env is not
            _ => fail!(
                GeneratorErrorKind::InconsistentWranglerBinding,
                "AST is missing WranglerEnv but models are defined"
            ),
        };

        // If D1 models are defined, ensure a D1 database binding exists
        ensure!(
            !ast.d1_models.is_empty() || self.d1_databases.is_empty(),
            GeneratorErrorKind::InconsistentWranglerBinding,
            "No D1 database binding was found, but D1 models were defined {}",
            env.source_path.display()
        );

        // TODO: multiple databases
        if let Some(db) = self.d1_databases.first() {
            ensure!(
                env.d1_binding == db.binding,
                GeneratorErrorKind::InconsistentWranglerBinding,
                "A Wrangler specification D1 binding did not match the WranglerEnv binding {}.{:?} != {} in {}",
                env.name,
                env.d1_binding,
                db.binding.as_ref().unwrap(),
                env.source_path.display()
            );
        }

        ensure!(
            !ast.kv_models.is_empty() || !self.kv_namespaces.is_empty(),
            GeneratorErrorKind::InconsistentWranglerBinding,
            "No KV namespace binding was found, but KV models were defined {}",
            env.source_path.display()
        );

        for kv in &env.kv_bindings {
            ensure!(
                self.kv_namespaces
                    .iter()
                    .any(|ns| ns.binding.as_ref().is_some_and(|b| b == kv)),
                GeneratorErrorKind::InconsistentWranglerBinding,
                "A Wrangler specification KV binding {} was missing or did not match the WranglerEnv binding at {}",
                kv,
                env.source_path.display()
            )
        }

        for var in self.vars.keys() {
            ensure!(
                env.vars.contains_key(var),
                GeneratorErrorKind::InconsistentWranglerBinding,
                "{} is defined in wrangler but not in the AST's WranglerEnv",
                var
            )
        }

        for var in env.vars.keys() {
            ensure!(
                self.vars.contains_key(var),
                GeneratorErrorKind::InconsistentWranglerBinding,
                "{} is defined in the AST's WranglerEnv but not in wrangler",
                var
            )
        }

        Ok(())
    }
}

/// Represents either a JSON or TOML Wrangler config, providing methods to
/// modify the original values without serializing the entire config
pub enum WranglerFormat {
    Json(JsonValue),
    Toml(TomlValue),
}

impl WranglerFormat {
    pub fn from_path(path: &Path) -> Self {
        let contents = std::fs::read_to_string(path).expect("Failed to open wrangler file");
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .expect("Missing or invalid extension");

        match extension {
            "json" => {
                let val: JsonValue =
                    serde_json::from_str(contents.as_str()).expect("JSON to be opened");
                WranglerFormat::Json(val)
            }
            "toml" => {
                let val: TomlValue = toml::from_str(&contents).expect("Toml to be opened");
                WranglerFormat::Toml(val)
            }
            other => panic!("Unsupported wrangler file extension: {other}"),
        }
    }

    pub fn update(&mut self, spec: WranglerSpec, mut wrangler_file: File) {
        match self {
            WranglerFormat::Json(val) => {
                val["d1_databases"] =
                    serde_json::to_value(&spec.d1_databases).expect("JSON to serialize");

                val["kv_namespaces"] =
                    serde_json::to_value(&spec.kv_namespaces).expect("JSON to serialize");

                val["vars"] = serde_json::to_value(&spec.vars).expect("JSON to serialize");

                // entrypoint + metadata (only if provided)
                if let Some(name) = &spec.name {
                    val["name"] = serde_json::to_value(name).expect("JSON to serialize");
                }
                if let Some(date) = &spec.compatibility_date {
                    val["compatibility_date"] =
                        serde_json::to_value(date).expect("JSON to serialize");
                }
                if let Some(main) = &spec.main {
                    val["main"] = serde_json::to_value(main).expect("JSON to serialize");
                }
            }
            WranglerFormat::Toml(val) => {
                if let toml::Value::Table(table) = val {
                    table.insert(
                        "d1_databases".to_string(),
                        toml::Value::try_from(&spec.d1_databases).expect("TOML to serialize"),
                    );

                    table.insert(
                        "vars".to_string(),
                        toml::Value::try_from(&spec.vars).expect("TOML to serialize"),
                    );

                    table.insert(
                        "kv_namespaces".to_string(),
                        toml::Value::try_from(&spec.kv_namespaces).expect("TOML to serialize"),
                    );

                    // entrypoint + metadata (only if provided)
                    if let Some(name) = &spec.name {
                        table.insert("name".to_string(), toml::Value::String(name.clone()));
                    }
                    if let Some(date) = &spec.compatibility_date {
                        table.insert(
                            "compatibility_date".to_string(),
                            toml::Value::String(date.clone()),
                        );
                    }
                    if let Some(main) = &spec.main {
                        table.insert("main".to_string(), toml::Value::String(main.clone()));
                    }
                } else {
                    panic!("Expected TOML root to be a table");
                }
            }
        }

        let data = match self {
            WranglerFormat::Json(val) => {
                serde_json::to_string_pretty(val).expect("JSON to serialize")
            }
            WranglerFormat::Toml(val) => toml::to_string_pretty(val).expect("TOML to serialize"),
        };

        let _ = wrangler_file
            .write(data.as_bytes())
            .expect("Failed to write data to the provided wrangler path");
    }

    /// Takes the entire Wrangler config and interprets only a [WranglerSpec]
    pub fn as_spec(&self) -> WranglerSpec {
        match self {
            WranglerFormat::Json(val) => {
                serde_json::from_value(val.clone()).expect("Failed to deserialize wrangler.json")
            }
            WranglerFormat::Toml(val) => {
                WranglerSpec::deserialize(val.clone()).expect("Failed to deserialize wrangler.toml")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, HashMap},
        path::PathBuf,
    };

    use ast::{
        KVModel, WranglerEnv,
        builder::{D1ModelBuilder, create_ast},
        err::GeneratorErrorKind,
    };

    use crate::WranglerFormat;

    #[test]
    fn test_serialize_wrangler_spec() {
        // Empty TOML
        {
            WranglerFormat::Toml(toml::from_str("").unwrap()).as_spec();
        }

        // Empty JSON
        {
            WranglerFormat::Json(serde_json::from_str("{}").unwrap()).as_spec();
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
                ("API_KEY".into(), ast::CidlType::Text),
                ("TIMEOUT".into(), ast::CidlType::Integer),
                ("ENABLED".into(), ast::CidlType::Boolean),
                ("THRESHOLD".into(), ast::CidlType::Real),
            ]
            .into_iter()
            .collect(),
            kv_bindings: vec![],
        });

        // Act
        let specs = vec![
            {
                let mut spec = WranglerFormat::Toml(toml::from_str("").unwrap()).as_spec();
                spec.generate_defaults(&ast);
                spec
            },
            {
                let mut spec = WranglerFormat::Json(serde_json::from_str("{}").unwrap()).as_spec();
                spec.generate_defaults(&ast);
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
        let mut ast = create_ast(vec![D1ModelBuilder::new("User").id().build()]);
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
                let mut spec = WranglerFormat::Toml(toml::from_str("").unwrap()).as_spec();
                spec.generate_defaults(&ast);
                spec
            },
            {
                let mut spec = WranglerFormat::Json(serde_json::from_str("{}").unwrap()).as_spec();
                spec.generate_defaults(&ast);
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
        let mut ast = create_ast(vec![]);
        ast.kv_models.insert(
            "MyKV".into(),
            KVModel {
                name: "MyKV".into(),
                binding: "my_kv".into(),
                cidl_type: ast::CidlType::JsonValue,
                methods: BTreeMap::default(),
                source_path: PathBuf::default(),
            },
        );
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
                let mut spec = WranglerFormat::Toml(toml::from_str("").unwrap()).as_spec();
                spec.generate_defaults(&ast);
                spec
            },
            {
                let mut spec = WranglerFormat::Json(serde_json::from_str("{}").unwrap()).as_spec();
                spec.generate_defaults(&ast);
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
    fn validate_missing_variable_in_wrangler() {
        // Arrange
        let mut ast = create_ast(vec![D1ModelBuilder::new("User").id().build()]);
        ast.wrangler_env = Some(WranglerEnv {
            name: "Env".into(),
            source_path: "source.ts".into(),
            d1_binding: None,
            kv_bindings: vec![],
            vars: [
                ("API_KEY".into(), ast::CidlType::Text),
                ("TIMEOUT".into(), ast::CidlType::Integer),
            ]
            .into_iter()
            .collect(),
        });

        let specs = vec![
            WranglerFormat::Toml(toml::from_str("").unwrap()).as_spec(),
            WranglerFormat::Json(serde_json::from_str("{}").unwrap()).as_spec(),
        ];

        // Act + Assert
        for spec in specs {
            assert!(matches!(
                spec.validate_ast_matches_wrangler(&ast).unwrap_err().kind,
                GeneratorErrorKind::InconsistentWranglerBinding
            ));
        }
    }

    #[test]
    fn validate_missing_env_in_ast() {
        // Arrange
        let ast = create_ast(vec![D1ModelBuilder::new("User").id().build()]);

        let specs = vec![
            WranglerFormat::Toml(toml::from_str("").unwrap()).as_spec(),
            WranglerFormat::Json(serde_json::from_str("{}").unwrap()).as_spec(),
        ];

        // Act + Assert
        for spec in specs {
            assert!(matches!(
                spec.validate_ast_matches_wrangler(&ast).unwrap_err().kind,
                GeneratorErrorKind::InconsistentWranglerBinding
            ));
        }
    }
}
