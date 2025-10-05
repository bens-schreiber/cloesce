use std::path::Path;
use std::{fs::File, io::Write};

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
    /// Ensures that all required values exist or places a default
    /// for them
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
                d1.binding = Some(format!("db_{i}"));
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
                binding: Some(String::from("db")),
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

                // entrypoint + metadata (only if provided)
                if let Some(name) = &spec.name {
                    val["name"] = serde_json::to_value(name).expect("JSON to serialize");
                }
                if let Some(date) = &spec.compatability_date {
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
}
