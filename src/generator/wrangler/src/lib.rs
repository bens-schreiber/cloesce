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
pub struct WranglerSpec {
    pub name: Option<String>,
    pub compatibility_date: Option<String>,
    pub main: Option<String>,

    #[serde(default)]
    pub d1_databases: Vec<D1Database>,

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

        // Validate existing database configs, filling in missing values with a default
        for (i, d1) in self.d1_databases.iter_mut().enumerate() {
            if d1.database_name.is_none() {
                d1.database_name = Some(format!("Cloesce_d1_{i}"));
                tracing::warn!("Created a default database Cloesce_d1_{i}")
            }

            if d1.binding.is_none() {
                d1.binding = Some(format!("db_{i}"));
                tracing::warn!("Created a default database binding db_{i}")
            }

            if d1.database_id.is_none() {
                d1.database_id = Some("replace_with_db_id".into());

                tracing::warn!(
                    "Database {} is missing an id. See https://developers.cloudflare.com/d1/get-started/",
                    d1.database_name.as_ref().unwrap()
                );
            }
        }

        // Ensure a database exists (if there are even models), provide a default if not
        if self.d1_databases.is_empty() {
            self.d1_databases.push(D1Database {
                binding: Some(String::from("db")),
                database_name: Some(String::from("default")),
                database_id: Some(String::from("replace_with_db_id")),
            });

            tracing::warn!(
                "Database \"default\" is missing an id. See https://developers.cloudflare.com/d1/get-started/"
            );
        }

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

    /// Validates that the bindings described in the AST's WranglerEnv are
    /// consistent with the wrangler spec
    pub fn validate_bindings(&self, ast: &CloesceAst) -> Result<()> {
        let env = if ast.models.len() > 0 {
            match &ast.wrangler_env {
                Some(env) => env,
                None => {
                    fail!(
                        GeneratorErrorKind::InconsistentWranglerBinding,
                        "AST is missing WranglerEnv but models are defined"
                    );
                }
            }
        } else {
            return Ok(());
        };

        if ast.models.len() > 0 {}

        // TODO: Multiple DB's
        let Some(db) = self.d1_databases.first() else {
            fail!(
                GeneratorErrorKind::InconsistentWranglerBinding,
                "No D1 databases defined in wrangler config for {}",
                env.source_path.display()
            );
        };

        ensure!(
            Some(&env.db_binding) == db.binding.as_ref(),
            GeneratorErrorKind::InconsistentWranglerBinding,
            "{}.{} != {} in {}",
            env.name,
            env.db_binding,
            db.binding.as_ref().unwrap(),
            env.source_path.display()
        );

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
    use ast::{WranglerEnv, builder::create_ast, err::GeneratorErrorKind};

    use crate::{D1Database, WranglerFormat};

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
            db_binding: "db".into(),
            vars: [
                ("API_KEY".into(), ast::CidlType::Text),
                ("TIMEOUT".into(), ast::CidlType::Integer),
                ("ENABLED".into(), ast::CidlType::Boolean),
                ("THRESHOLD".into(), ast::CidlType::Real),
            ]
            .into_iter()
            .collect(),
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
            assert_eq!(spec.d1_databases.len(), 1);
            assert_eq!(spec.d1_databases[0].binding.as_ref().unwrap(), "db");
            assert_eq!(
                spec.d1_databases[0].database_name.as_ref().unwrap(),
                "default"
            );
            assert_eq!(spec.vars.get("API_KEY").unwrap(), "default_string");
            assert_eq!(spec.vars.get("TIMEOUT").unwrap(), "0");
            assert_eq!(*spec.vars.get("ENABLED").unwrap(), "false");
            assert_eq!(*spec.vars.get("THRESHOLD").unwrap(), "0");
        }
    }

    #[test]
    fn validate_missing_variable_in_wrangler() {
        // Arrange
        let mut ast = create_ast(vec![]);
        ast.wrangler_env = Some(WranglerEnv {
            name: "Env".into(),
            source_path: "source.ts".into(),
            db_binding: "db".into(),
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
        for mut spec in specs {
            spec.d1_databases.push(D1Database {
                binding: Some("db".into()),
                database_name: Some("default".into()),
                database_id: Some("".into()),
            });

            assert!(matches!(
                spec.validate_bindings(&ast).unwrap_err().kind,
                GeneratorErrorKind::InconsistentWranglerBinding
            ));
        }
    }
}
