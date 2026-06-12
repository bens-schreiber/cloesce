use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod migrations;
mod spec;

pub use migrations::DurableMigrationGenerator;
pub use spec::{WranglerDefault, WranglerGenerator};

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct D1Database {
    pub binding: Option<String>,
    pub database_name: Option<String>,
    pub database_id: Option<String>,
    pub migrations_dir: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct KVNamespace {
    pub binding: Option<String>,
    pub id: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct R2Bucket {
    pub binding: Option<String>,
    pub bucket_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DurableObjectBinding {
    pub name: Option<String>,
    pub class_name: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DurableObjects {
    #[serde(default)]
    pub bindings: Vec<DurableObjectBinding>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct RenamedClass {
    pub from: String,
    pub to: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct DurableObjectMigration {
    pub tag: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new_sqlite_classes: Vec<String>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renamed_classes: Vec<RenamedClass>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deleted_classes: Vec<String>,
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
    pub r2_buckets: Vec<R2Bucket>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub durable_objects: Option<DurableObjects>,

    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub migrations: Vec<DurableObjectMigration>,

    #[serde(default)]
    pub vars: HashMap<String, Value>,
}
