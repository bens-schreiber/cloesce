use std::path::{Path, PathBuf};
use std::{fs::File, io::Write};

use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

#[derive(Serialize, Deserialize, Debug)]
pub enum CidlType {
    Integer,
    Real,
    Text,
    Blob,
    D1Database,
    Model(String),
    Array(Box<CidlType>),
}

#[derive(Serialize, Deserialize)]
pub enum HttpVerb {
    GET,
    POST,
    PUT,
    PATCH,
    DELETE,
}

#[derive(Serialize, Deserialize)]
pub struct TypedValue {
    pub name: String,
    pub cidl_type: CidlType,
    pub nullable: bool,
}

#[derive(Serialize, Deserialize)]
pub enum ForeignKey {
    ManyToMany(String),
    OneToOne(String),
    OneToMany(String),
}

#[derive(Serialize, Deserialize)]
pub struct Attribute {
    pub value: TypedValue,
    pub primary_key: bool,
    pub foreign_key: Option<ForeignKey>,
}

#[derive(Serialize, Deserialize)]
pub struct Method {
    pub name: String,
    pub is_static: bool,
    pub http_verb: HttpVerb,
    pub parameters: Vec<TypedValue>,
}

#[derive(Serialize, Deserialize)]
pub enum IncludeTree {
    Node {
        value: TypedValue,
        tree: Vec<IncludeTree>,
    },
    None,
}

#[derive(Serialize, Deserialize)]
pub struct DataSource {
    name: String,
    tree: IncludeTree,
}

#[derive(Serialize, Deserialize)]
pub struct Model {
    pub name: String,
    pub attributes: Vec<Attribute>,
    pub methods: Vec<Method>,
    pub data_sources: Vec<DataSource>,
    pub source_path: PathBuf,
}

#[derive(Serialize, Deserialize)]
pub enum InputLanguage {
    TypeScript,
}

#[derive(Serialize, Deserialize)]
pub struct CidlSpec {
    pub version: String,
    pub project_name: String,
    pub language: InputLanguage,
    pub models: Vec<Model>,
}

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
