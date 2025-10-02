use std::path::Path;
use std::{fs::File, io::Write};

use anyhow::{anyhow, Context, Result};
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
    pub compatability_date: Option<String>,
    pub main: Option<String>,

    #[serde(default)]
    pub d1_databases: Vec<D1Database>,
}

impl WranglerSpec {
    pub fn generate_defaults(&mut self) {
        // Generate default worker entry point values
        self.name = Some(self.name.clone().unwrap_or_else(|| "cloesce".to_string()));

        self.compatability_date = Some(
            self.compatability_date
                .clone()
                .unwrap_or_else(|| "2025-10-02".to_string()),
        );

        self.main = Some(
            self.main
                .clone()
                .unwrap_or_else(|| "workers.ts".to_string()),
        );

        // Validate existing database configs, filling in missing values with a default
        for (i, d1) in self.d1_databases.iter_mut().enumerate() {
            if d1.binding.is_none() {
                d1.binding = Some(format!("D1_DB_{i}"));
            }

            if d1.database_name.is_none() {
                d1.database_name = Some(format!("{}_d1_{i}", "Cloesce"));
            }

            if d1.database_id.is_none() {
                d1.database_id = Some("replace_with_db_id".into());

                eprintln!(
                    "Warning: Database \"default\" is missing an id. \n https://developers.cloudflare.com/d1/get-started/"
                );
            }
        }

        // Ensure a database exists (if there are even models), provide a default if not
        if self.d1_databases.is_empty() {
            self.d1_databases.push(D1Database {
                binding: Some(String::from("D1_DB")),
                database_name: Some(String::from("default")),
                database_id: Some(String::from("replace_with_db_id")),
            });

            eprintln!(
                "Warning: Database \"default\" is missing an id. \n https://developers.cloudflare.com/d1/get-started/"
            );
        }
    }
}

/// Represents either a JSON or TOML Wrangler config, providing methods to
/// modify the original values without serializing the entire config
pub enum WranglerFormat {
    Json(JsonValue),
    Toml(TomlValue),
}

impl WranglerFormat {
    pub fn from_path(path: &Path) -> Result<Self> {
        let contents = std::fs::read_to_string(path).context("Failed to open wrangler file")?;
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or_else(|| anyhow::anyhow!("Missing or invalid extension"))?;

        match extension {
            "json" => {
                let val: JsonValue = serde_json::from_str(contents.as_str())?;
                Ok(WranglerFormat::Json(val))
            }
            "toml" => {
                let val: TomlValue = toml::from_str(&contents)?;
                Ok(WranglerFormat::Toml(val))
            }
            _ => Err(anyhow!(
                "Unsupported Wrangler format (expected .json or .toml)"
            )),
        }
    }

    pub fn update(&mut self, spec: WranglerSpec, mut wrangler_file: File) -> Result<()> {
        match self {
            WranglerFormat::Json(val) => {
                val["d1_databases"] = serde_json::to_value(&spec.d1_databases)?;

                // entrypoint + metadata (only if provided)
                if let Some(name) = &spec.name {
                    val["name"] = serde_json::to_value(name)?;
                }
                if let Some(date) = &spec.compatability_date {
                    val["compatibility_date"] = serde_json::to_value(date)?;
                }
                if let Some(main) = &spec.main {
                    val["main"] = serde_json::to_value(main)?;
                }
            }
            WranglerFormat::Toml(val) => {
                if let toml::Value::Table(table) = val {
                    table.insert(
                        "d1_databases".to_string(),
                        toml::Value::try_from(&spec.d1_databases)?,
                    );

                    // entrypoint + metadata (only if provided)
                    if let Some(name) = &spec.name {
                        table.insert("name".to_string(), toml::Value::String(name.clone()));
                    }
                    if let Some(date) = &spec.compatability_date {
                        table.insert(
                            "compatibility_date".to_string(),
                            toml::Value::String(date.clone()),
                        );
                    }
                    if let Some(main) = &spec.main {
                        table.insert("main".to_string(), toml::Value::String(main.clone()));
                    }
                } else {
                    return Err(anyhow!("Expected TOML root to be a table"));
                }
            }
        }

        let data = match self {
            WranglerFormat::Json(val) => serde_json::to_string_pretty(val)?,
            WranglerFormat::Toml(val) => toml::to_string_pretty(val)?,
        };

        wrangler_file
            .write(data.as_bytes())
            .context("Failed to write data to the provided wrangler path")?;

        Ok(())
    }

    /// Takes the entire Wrangler config and interprets only a [WranglerSpec]
    pub fn as_spec(&self) -> Result<WranglerSpec> {
        match self {
            WranglerFormat::Json(val) => {
                serde_json::from_value(val.clone()).context("Failed to deserialize wrangler.json")
            }
            WranglerFormat::Toml(val) => WranglerSpec::deserialize(val.clone())
                .context("Failed to deserialize wrangler.toml"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::WranglerFormat;

    #[test]
    fn test_serialize_wrangler_spec() {
        // Filled TOML
        {
            let wrangler_path = PathBuf::from("../../test_fixtures/wrangler.toml");
            WranglerFormat::from_path(&wrangler_path).expect("Wrangler file to serialize");
        }

        // Empty TOML
        {
            WranglerFormat::Toml(toml::from_str("").unwrap())
                .as_spec()
                .expect("Wrangler file to serialize");
        }

        // Filled JSON
        {
            let wrangler_path = PathBuf::from("../../test_fixtures/wrangler.json");
            WranglerFormat::from_path(&wrangler_path).expect("Wrangler file to serialize");
        }

        // Empty JSON
        {
            WranglerFormat::Json(serde_json::from_str("{}").unwrap())
                .as_spec()
                .expect("Wrangler file to serialize");
        }
    }
}

// pub mod sql;

// use common::{
//     CloesceAst,
//     wrangler::{D1Database, WranglerSpec},
// };
// use sql::generate_sql;

// use anyhow::Result;

// pub struct D1Generator {
//     ast: CloesceAst,
//     wrangler: WranglerSpec,
// }

// impl D1Generator {
//     pub fn new(ast: CloesceAst, wrangler: WranglerSpec) -> Self {
//         Self { ast, wrangler }
//     }

//     /// Validates and updates the Wrangler spec so that D1 can be used during
//     /// code generation.
//     pub fn wrangler(&self) -> WranglerSpec {
//         // Validate existing database configs, filling in missing values with a default
//         let mut res = self.wrangler.clone();
//         for (i, d1) in res.d1_databases.iter_mut().enumerate() {
//             if d1.binding.is_none() {
//                 d1.binding = Some(format!("D1_DB_{i}"));
//             }

//             if d1.database_name.is_none() {
//                 d1.database_name = Some(format!("{}_d1_{i}", self.ast.project_name));
//             }

//             if d1.database_id.is_none() {
//                 eprintln!(
//                     "Warning: Database \"{}\" is missing an id. \n https://developers.cloudflare.com/d1/get-started/",
//                     d1.database_name.as_ref().unwrap()
//                 )
//             }
//         }

//         // Ensure a database exists (if there are even models), provide a default if not
//         if !self.ast.models.is_empty() && res.d1_databases.is_empty() {
//             res.d1_databases.push(D1Database {
//                 binding: Some(String::from("D1_DB")),
//                 database_name: Some(String::from("default")),
//                 database_id: None,
//             });

//             eprintln!(
//                 "Warning: Database \"default\" is missing an id. \n https://developers.cloudflare.com/d1/get-started/"
//             );
//         }

//         res
//     }

//     /// Transforms the Model AST into their SQL table equivalents
//     pub fn sql(&self) -> Result<String> {
//         generate_sql(&self.ast.models)
//     }
// }
