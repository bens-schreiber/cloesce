use std::path::Path;
use std::{fs::File, io::Write};

use anyhow::{Context, Result, anyhow};
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
    #[serde(default)]
    pub d1_databases: Vec<D1Database>,
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

    /// Updates the original Wrangler config for values within the [WranglerSpec]
    pub fn update(&mut self, updated: &WranglerSpec, mut wrangler_file: File) -> Result<()> {
        match self {
            WranglerFormat::Json(val) => {
                val["d1_databases"] = serde_json::to_value(&updated.d1_databases)?;
            }
            WranglerFormat::Toml(val) => {
                if let toml::Value::Table(table) = val {
                    table.insert(
                        "d1_databases".to_string(),
                        toml::Value::try_from(&updated.d1_databases)?,
                    );
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
