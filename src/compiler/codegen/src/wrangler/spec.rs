use std::path::Path;

use idl::{CidlType, CloesceIdl};
use serde_json::{Map, Value as JsonValue};

use crate::wrangler::{
    D1Database, DurableObjectBinding, DurableObjectMigration, DurableObjects, KVNamespace,
    R2Bucket, WranglerSpec,
};

#[derive(Clone, Copy)]
pub enum Format {
    Json,
    Toml,
}

/// A wrangler config held as a canonical JSON document, regardless of whether
/// it was authored as JSON or TOML.
pub struct WranglerGenerator {
    root: JsonValue,
    format: Format,
}

fn entry_object<'a>(
    obj: &'a mut Map<String, JsonValue>,
    key: &str,
    ctx: &str,
) -> &'a mut Map<String, JsonValue> {
    obj.entry(key.to_string())
        .or_insert_with(|| JsonValue::Object(Map::new()))
        .as_object_mut()
        .unwrap_or_else(|| panic!("{ctx}"))
}

/// Merge `items` into the `key` array under `target`, matching existing entries
/// by their `match_field` string member. For a match, `patch` applies per-field
/// updates onto the existing entry; otherwise the whole item is appended.
fn merge_into_array<T: serde::Serialize>(
    target: &mut Map<String, JsonValue>,
    key: &str,
    match_field: &str,
    items: &[T],
    matched: impl Fn(&T) -> Option<&str>,
    patch: impl Fn(&T, &mut Map<String, JsonValue>),
) {
    let arr = target
        .entry(key.to_string())
        .or_insert_with(|| JsonValue::Array(vec![]))
        .as_array_mut()
        .expect("entry must be an array");

    for item in items {
        let id = matched(item);
        if let Some(JsonValue::Object(existing)) = arr
            .iter_mut()
            .find(|e| e.get(match_field).and_then(|b| b.as_str()) == id)
        {
            patch(item, existing);
        } else {
            arr.push(serde_json::to_value(item).expect("JSON to serialize"));
        }
    }
}

impl WranglerGenerator {
    // Generic string error is sufficient because this is only used in the CLI,
    // which doesn't need to distinguish error types
    pub fn from_contents(contents: String, path: &Path) -> Result<WranglerGenerator, String> {
        let extension = path
            .extension()
            .and_then(|e| e.to_str())
            .ok_or("Wrangler file extension is not valid UTF-8".to_string())?;

        match extension {
            "json" | "jsonc" => {
                let contents_no_comments = json_comments::StripComments::new(contents.as_bytes());
                let root: JsonValue = serde_json::from_reader(contents_no_comments)
                    .unwrap_or(JsonValue::Object(Map::new()));
                Ok(WranglerGenerator {
                    root,
                    format: Format::Json,
                })
            }
            "toml" => {
                // Reinterpret TOML as the canonical JSON document.
                let toml_val: toml::Value =
                    toml::from_str(&contents).unwrap_or(toml::Value::Table(toml::Table::new()));
                let root = serde_json::to_value(toml_val).expect("TOML reparses as JSON");
                Ok(WranglerGenerator {
                    root,
                    format: Format::Toml,
                })
            }
            other => Err(format!("Unsupported wrangler file extension: {other}")),
        }
    }

    fn root_object(&mut self) -> &mut Map<String, JsonValue> {
        self.root
            .as_object_mut()
            .expect("Expected wrangler root to be an object")
    }

    pub fn generate(&mut self, spec: WranglerSpec, env: Option<&str>) -> String {
        // Top-level fields (name, compatibility_date, main) always go at root
        if let Some(name) = &spec.name {
            self.insert("name", name.clone());
        }

        if let Some(date) = &spec.compatibility_date {
            self.insert("compatibility_date", date.clone());
        }

        if let Some(main) = &spec.main {
            self.insert("main", main.clone());
        }

        Self::write_bindings(self.root_object(), &spec, env);

        match self.format {
            Format::Json => serde_json::to_string_pretty(&self.root).expect("JSON to serialize"),
            Format::Toml => {
                let toml_val = toml::Value::try_from(&self.root).expect("JSON reparses as TOML");
                toml::to_string_pretty(&toml_val).expect("TOML to serialize")
            }
        }
    }

    /// Writes every binding section of `spec` into `root`.
    fn write_bindings(root: &mut Map<String, JsonValue>, spec: &WranglerSpec, env: Option<&str>) {
        // When an env is specified, write bindings into env.<name> instead of root
        let target = match env {
            Some(env_name) => {
                let envs = entry_object(root, "env", "env must be an object");
                entry_object(envs, env_name, "env entry must be an object")
            }
            None => root,
        };

        if !spec.d1_databases.is_empty() {
            merge_into_array(
                target,
                "d1_databases",
                "binding",
                &spec.d1_databases,
                |db| db.binding.as_deref(),
                |db, existing| {
                    if let Some(id) = &db.database_id {
                        existing.insert("database_id".into(), id.clone().into());
                    }
                    if let Some(name) = &db.database_name {
                        existing.insert("database_name".into(), name.clone().into());
                    }
                    if let Some(migrations_dir) = &db.migrations_dir {
                        existing.insert("migrations_dir".into(), migrations_dir.clone().into());
                    }
                },
            );
        }

        if !spec.kv_namespaces.is_empty() {
            merge_into_array(
                target,
                "kv_namespaces",
                "binding",
                &spec.kv_namespaces,
                |ns| ns.binding.as_deref(),
                |ns, existing| {
                    if let Some(id) = &ns.id {
                        existing.insert("id".into(), id.clone().into());
                    }
                },
            );
        }

        if !spec.r2_buckets.is_empty() {
            merge_into_array(
                target,
                "r2_buckets",
                "binding",
                &spec.r2_buckets,
                |bucket| bucket.binding.as_deref(),
                |bucket, existing| {
                    if let Some(name) = &bucket.bucket_name {
                        existing.insert("bucket_name".into(), name.clone().into());
                    }
                },
            );
        }

        if let Some(durable_objects) = &spec.durable_objects
            && !durable_objects.bindings.is_empty()
        {
            let dos = entry_object(
                target,
                "durable_objects",
                "durable_objects must be an object",
            );
            merge_into_array(
                dos,
                "bindings",
                "name",
                &durable_objects.bindings,
                |binding| binding.name.as_deref(),
                |binding, existing| {
                    if let Some(class_name) = &binding.class_name {
                        existing.insert("class_name".into(), class_name.clone().into());
                    }
                },
            );
        }

        if !spec.migrations.is_empty() {
            target.insert(
                "migrations".into(),
                serde_json::to_value(&spec.migrations).expect("JSON to serialize"),
            );
        }

        if !spec.vars.is_empty() {
            target.insert(
                "vars".into(),
                serde_json::to_value(&spec.vars).expect("JSON to serialize"),
            );
        }
    }

    /// Takes the entire Wrangler config and interprets only a [WranglerSpec].
    /// When `env` is specified, reads bindings from `env.<name>`, merging
    /// top-level fields (name, compatibility_date, main) that are absent there.
    pub fn as_spec(&self, env: Option<&str>) -> Result<WranglerSpec, Box<dyn std::error::Error>> {
        let Some(env_name) = env else {
            return Ok(serde_json::from_value(self.root.clone())?);
        };

        // Start from the env-specific subobject, falling back to top-level
        let env_val = self
            .root
            .get("env")
            .and_then(|e| e.get(env_name))
            .cloned()
            .unwrap_or(JsonValue::Object(Map::new()));

        let mut merged = match env_val {
            JsonValue::Object(map) => map,
            _ => Map::new(),
        };

        // Inherit top-level scalar fields when absent in the env block
        for key in &["name", "compatibility_date", "main"] {
            if !merged.contains_key(*key)
                && let Some(v) = self.root.get(*key)
            {
                merged.insert(key.to_string(), v.clone());
            }
        }

        Ok(serde_json::from_value(JsonValue::Object(merged))?)
    }

    pub fn insert(&mut self, key: &str, value: impl Into<JsonValue>) {
        self.root_object().insert(key.to_string(), value.into());
    }
}

pub struct WranglerDefault;
impl WranglerDefault {
    /// Ensures that all required values exist or places a default
    /// for them
    pub fn set_defaults(spec: &mut WranglerSpec, idl: &CloesceIdl, default_migrations_path: &str) {
        let default_migrations_path = default_migrations_path
            .trim_end_matches('/')
            .trim_end_matches('\\');
        let default_migrations_path = if default_migrations_path.is_empty() {
            "migrations"
        } else {
            default_migrations_path
        };

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
                .unwrap_or_else(|| "workers.ts".to_string()), // TODO: non-hardcoded default
        );

        // Ensure all bindings referenced in the WranglerEnv exist in the spec
        for d1 in &idl.wrangler_env.d1_bindings {
            let db = spec
                .d1_databases
                .iter_mut()
                .find(|db| db.binding.as_deref() == Some(d1));

            match db {
                Some(db) => {
                    if db.database_id.is_none() {
                        db.database_id = Some(format!("replace_with_{}_id", d1));
                        tracing::warn!(
                            "D1 Database with binding {} is missing an id. See https://developers.cloudflare.com/d1/get-started/",
                            d1
                        );
                    }
                    if db.database_name.is_none() {
                        db.database_name = Some(format!("replace_with_{}_name", d1));
                        tracing::warn!(
                            "D1 Database with binding {} is missing a name. See https://developers.cloudflare.com/d1/get-started/",
                            d1
                        );
                    }
                    if db.migrations_dir.is_none() {
                        db.migrations_dir = Some(format!("{}/{}", default_migrations_path, d1));
                        tracing::warn!(
                            "D1 Database with binding {} is missing a migrations_dir. Defaulting to {}/{}",
                            d1,
                            default_migrations_path,
                            d1
                        );
                    }
                }
                None => {
                    spec.d1_databases.push(D1Database {
                        binding: Some(d1.to_string()),
                        database_name: Some(format!("replace_with_{}_name", d1)),
                        database_id: Some(format!("replace_with_{}_id", d1)),
                        migrations_dir: Some(format!("{}/{}", default_migrations_path, d1)),
                    });

                    tracing::warn!(
                        "D1 Database with binding {} was missing, added a default. See https://developers.cloudflare.com/d1/get-started/",
                        d1
                    );
                }
            }
        }

        for kv_binding in &idl.wrangler_env.kv_bindings {
            let name = kv_binding.name;
            let kv = spec
                .kv_namespaces
                .iter_mut()
                .find(|ns| ns.binding.as_deref() == Some(name));

            match kv {
                Some(ns) => {
                    if ns.id.is_none() {
                        ns.id = Some(format!("replace_with_{}_id", name));
                        tracing::warn!(
                            "KV Namespace with binding {} is missing an id. See https://developers.cloudflare.com/workers/platform/storage/#namespaces",
                            name
                        );
                    }
                }
                None => {
                    spec.kv_namespaces.push(KVNamespace {
                        binding: Some(name.to_string()),
                        id: Some(format!("replace_with_{}_id", name)),
                    });

                    tracing::warn!(
                        "KV Namespace with binding {} was missing, added a default. See https://developers.cloudflare.com/workers/platform/storage/#namespaces",
                        name
                    );
                }
            }
        }

        for r2_binding in &idl.wrangler_env.r2_bindings {
            let name = r2_binding.name;
            let r2 = spec
                .r2_buckets
                .iter_mut()
                .find(|bucket| bucket.binding.as_deref() == Some(name));

            match r2 {
                Some(bucket) => {
                    if bucket.bucket_name.is_none() {
                        bucket.bucket_name =
                            Some(format!("replace-with-{}-name", name).to_lowercase());
                        tracing::warn!(
                            "R2 Bucket with binding {} is missing a bucket name. See https://developers.cloudflare.com/r2/get-started/",
                            name
                        );
                    }
                }
                None => {
                    spec.r2_buckets.push(R2Bucket {
                        binding: Some(name.to_string()),
                        bucket_name: Some(format!("replace-with-{}-name", name).to_lowercase()),
                    });

                    tracing::warn!(
                        "R2 Bucket with binding {} was missing, added a default. See https://developers.cloudflare.com/r2/get-started/",
                        name
                    );
                }
            }
        }

        for durable_binding in &idl.wrangler_env.durable_bindings {
            let name = durable_binding.name;
            let durable_objects = spec
                .durable_objects
                .get_or_insert_with(DurableObjects::default);

            let existing = durable_objects
                .bindings
                .iter_mut()
                .find(|b| b.name.as_deref() == Some(name));

            match existing {
                Some(binding) => {
                    if binding.class_name.is_none() {
                        binding.class_name = Some(name.to_string());
                    }
                }
                None => {
                    durable_objects.bindings.push(DurableObjectBinding {
                        name: Some(name.to_string()),
                        class_name: Some(name.to_string()),
                    });
                }
            }
        }

        // Wrangler DO class migrations: fold the existing `[[migrations]]` entries into
        // the set of live classes, then register any schema DO binding not yet covered
        // under a new tag. Renames and deletions are destructive and left to the
        // developer; hand-authored entries participate in the fold.
        {
            let mut live_classes: Vec<String> = vec![];
            for migration in &spec.migrations {
                live_classes.extend(migration.new_sqlite_classes.iter().cloned());
                for rename in &migration.renamed_classes {
                    if let Some(class) = live_classes.iter_mut().find(|c| **c == rename.from) {
                        *class = rename.to.clone();
                    }
                }
                live_classes.retain(|c| !migration.deleted_classes.contains(c));
            }

            let new_classes = idl
                .wrangler_env
                .durable_bindings
                .iter()
                .map(|b| b.name.to_string())
                .filter(|name| !live_classes.contains(name))
                .collect::<Vec<_>>();

            if !new_classes.is_empty() {
                spec.migrations.push(DurableObjectMigration {
                    tag: format!("v{}", spec.migrations.len() + 1),
                    new_sqlite_classes: new_classes,
                    ..Default::default()
                });
            }

            for stale in live_classes.iter().filter(|c| {
                !idl.wrangler_env
                    .durable_bindings
                    .iter()
                    .any(|b| b.name == c.as_str())
            }) {
                tracing::warn!(
                    "Durable Object class {} is registered in the wrangler migrations but has no binding in the schema. \
                    To delete it, add a migration entry with deleted_classes = [\"{}\"]",
                    stale,
                    stale
                );
            }
        }

        // Generate default vars from the IDL's WranglerEnv
        for var in &idl.wrangler_env.vars {
            spec.vars.entry(var.name.to_string()).or_insert_with(|| {
                let default = match var.cidl_type {
                    CidlType::String => "default_string",
                    CidlType::Int | CidlType::Real => "0",
                    CidlType::Boolean => "false",
                    _ => "default_value",
                };
                tracing::warn!(
                    "Added missing Wrangler var {} with a default value",
                    var.name
                );
                default.into()
            });
        }
    }
}
