use std::path::Path;
use std::{fs::File, io::Write};

use ast::{CidlType, CloesceAst, D1Database, KVNamespace, WranglerSpec};

use serde::Deserialize;
use serde_json::Value as JsonValue;
use toml::Value as TomlValue;

/// Represents either a JSON or TOML Wrangler config, providing methods to
/// modify the original values without serializing the entire config
pub enum WranglerGenerator {
    Json(JsonValue),
    Toml(TomlValue),
}

impl WranglerGenerator {
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
                WranglerGenerator::Json(val)
            }
            "toml" => {
                let val: TomlValue = toml::from_str(&contents).expect("Toml to be opened");
                WranglerGenerator::Toml(val)
            }
            other => panic!("Unsupported wrangler file extension: {other}"),
        }
    }

    pub fn generate(&mut self, spec: WranglerSpec, mut wrangler_file: File) {
        match self {
            WranglerGenerator::Json(val) => {
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

                if !spec.d1_databases.is_empty() {
                    val["d1_databases"] =
                        serde_json::to_value(&spec.d1_databases).expect("JSON to serialize");
                }

                if !spec.kv_namespaces.is_empty() {
                    val["kv_namespaces"] =
                        serde_json::to_value(&spec.kv_namespaces).expect("JSON to serialize");
                }

                if !spec.r2_buckets.is_empty() {
                    val["r2_buckets"] =
                        serde_json::to_value(&spec.kv_namespaces).expect("JSON to serialize");
                }

                if !spec.vars.is_empty() {
                    val["vars"] = serde_json::to_value(&spec.vars).expect("JSON to serialize");
                }
            }
            WranglerGenerator::Toml(val) => {
                if let toml::Value::Table(table) = val {
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

                    if !spec.d1_databases.is_empty() {
                        table.insert(
                            "d1_databases".to_string(),
                            toml::Value::try_from(&spec.d1_databases).expect("TOML to serialize"),
                        );
                    }

                    if !spec.kv_namespaces.is_empty() {
                        table.insert(
                            "kv_namespaces".to_string(),
                            toml::Value::try_from(&spec.kv_namespaces).expect("TOML to serialize"),
                        );
                    }

                    if !spec.r2_buckets.is_empty() {
                        table.insert(
                            "r2_buckets".to_string(),
                            toml::Value::try_from(&spec.r2_buckets).expect("TOML to serialize"),
                        );
                    }

                    if !spec.vars.is_empty() {
                        table.insert(
                            "vars".to_string(),
                            toml::Value::try_from(&spec.vars).expect("TOML to serialize"),
                        );
                    }
                } else {
                    panic!("Expected TOML root to be a table");
                }
            }
        }

        let data = match self {
            WranglerGenerator::Json(val) => {
                serde_json::to_string_pretty(val).expect("JSON to serialize")
            }
            WranglerGenerator::Toml(val) => toml::to_string_pretty(val).expect("TOML to serialize"),
        };

        let _ = wrangler_file
            .write(data.as_bytes())
            .expect("Failed to write data to the provided wrangler path");
    }

    /// Takes the entire Wrangler config and interprets only a [WranglerSpec]
    pub fn as_spec(&self) -> WranglerSpec {
        match self {
            WranglerGenerator::Json(val) => {
                serde_json::from_value(val.clone()).expect("Failed to deserialize wrangler.json")
            }
            WranglerGenerator::Toml(val) => {
                WranglerSpec::deserialize(val.clone()).expect("Failed to deserialize wrangler.toml")
            }
        }
    }
}

pub struct WranglerDefault;
impl WranglerDefault {
    /// Ensures that all required values exist or places a default
    /// for them
    pub fn set_defaults(spec: &mut WranglerSpec, ast: &CloesceAst) {
        // Generate default worker entry point values
        spec.name = Some(spec.name.clone().unwrap_or_else(|| {
            tracing::warn!("Set a default worker name \"cloesce\"");
            "cloesce".to_string()
        }));

        spec.compatibility_date = Some(spec.compatibility_date.clone().unwrap_or_else(|| {
            tracing::warn!("Set a default compatibility date.");
            "2025-10-02".to_string()
        }));

        spec.main = Some(
            spec.main
                .clone()
                .unwrap_or_else(|| "workers.ts".to_string()),
        );

        // Ensure all bindings referenced in the WranglerEnv exist in the spec
        if let Some(env) = &ast.wrangler_env {
            if let Some(db_binding) = &env.d1_binding {
                let db = spec
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
                        spec.d1_databases.push(D1Database {
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
                let kv = spec
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
                        spec.kv_namespaces.push(KVNamespace {
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

            for r2_binding in &env.r2_bindings {
                let r2 = spec
                    .r2_buckets
                    .iter_mut()
                    .find(|bucket| bucket.binding.as_deref() == Some(r2_binding));

                match r2 {
                    Some(bucket) => {
                        if bucket.bucket_name.is_none() {
                            bucket.bucket_name = Some("replace-with-r2-bucket-name".into());
                            tracing::warn!(
                                "R2 Bucket with binding {} is missing a bucket name. See https://developers.cloudflare.com/r2/get-started/",
                                r2_binding
                            );
                        }
                    }
                    None => {
                        spec.r2_buckets.push(ast::R2Bucket {
                            binding: Some(r2_binding.clone()),
                            bucket_name: Some("replace-with-r2-bucket-name".into()),
                        });

                        tracing::warn!(
                            "R2 Bucket with binding {} was missing, added a default. See https://developers.cloudflare.com/r2/get-started/",
                            r2_binding
                        );
                    }
                }
            }
        }

        // Generate default vars from the AST's WranglerEnv
        if let Some(env) = &ast.wrangler_env {
            for (var, ty) in &env.vars {
                spec.vars.entry(var.clone()).or_insert_with(|| {
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
}
