//! Test harness: turn an IDL into a set of migrated SQLite pools ([Backends]) and seed
//! them with plain INSERTs.
//!
//! Schema is applied **per database**: `uses_sqlite()` models are grouped by their
//! backing binding, one migration is generated per group, and it is applied to the
//! matching pool — and to every shard pool of a Durable Object binding.

use std::collections::HashMap;

use idl::{BackingKind, CloesceIdl, Model};
use migrations::{MigrationsGenerator, MigrationsIdl, MigrationsModel};
use serde_json::Value;
use sqlx::SqlitePool;

use super::executor::Backends;

struct MockIntent;
impl migrations::MigrationsIntent for MockIntent {
    fn ask(&self, _: migrations::MigrationsDilemma) -> Option<usize> {
        panic!("no migration dilemmas expected in tests")
    }
}

/// Generate the schema migration for a set of models sharing one database.
fn migration_for(models: &[&Model<'_>], hash: u64) -> String {
    let migrations_models = models
        .iter()
        .map(|model| {
            (
                model.name.to_string(),
                MigrationsModel {
                    hash: model.hash,
                    name: model.name.to_string(),
                    backing: None,
                    primary_columns: clone_columns(&model.primary_columns),
                    columns: clone_columns(&model.columns),
                },
            )
        })
        .collect();

    let idl = MigrationsIdl {
        hash,
        models: migrations_models,
    };
    MigrationsGenerator::migrate(&idl, None, &MockIntent)
}

/// `Column` is not `Clone`; round-trip through JSON to duplicate it for the migration IDL.
fn clone_columns<'src>(columns: &[idl::Column<'src>]) -> Vec<idl::Column<'src>> {
    columns
        .iter()
        .map(|c| {
            let json = serde_json::to_string(c).unwrap();
            serde_json::from_str(Box::leak(json.into_boxed_str())).unwrap()
        })
        .collect()
}

/// Which shard tuples to instantiate for each Durable Object binding. D1 bindings need
/// no entry; every DO binding that a plan touches must be listed.
pub type ShardSpec = HashMap<&'static str, Vec<Vec<Value>>>;

/// Build fully-migrated [Backends] for `idl`. Each D1 binding gets one pool; each DO
/// binding gets one pool per shard tuple in `shards`.
pub async fn setup(idl: &CloesceIdl<'_>, shards: &ShardSpec) -> Backends {
    let mut by_binding: HashMap<&str, (BackingKind, Vec<&Model>)> = HashMap::new();
    for model in idl.models.values() {
        if !model.uses_sqlite() {
            continue;
        }
        let Some(backing) = model.backing.as_ref() else {
            continue;
        };
        by_binding
            .entry(backing.binding)
            .or_insert_with(|| (backing.kind.clone(), Vec::new()))
            .1
            .push(model);
    }

    let mut d1 = HashMap::new();
    let mut durable: HashMap<String, HashMap<Vec<Value>, SqlitePool>> = HashMap::new();

    for (binding, (kind, models)) in by_binding {
        let migration = migration_for(&models, idl.hash);
        match kind {
            BackingKind::D1 => {
                let pool = new_pool(&migration).await;
                d1.insert(binding.to_string(), pool);
            }
            BackingKind::DurableObject => {
                let tuples = shards
                    .get(binding)
                    .unwrap_or_else(|| panic!("no shard spec for DO binding {binding}"));
                let mut instances = HashMap::new();
                for tuple in tuples {
                    instances.insert(tuple.clone(), new_pool(&migration).await);
                }
                durable.insert(binding.to_string(), instances);
            }
        }
    }

    Backends { d1, durable }
}

async fn new_pool(migration: &str) -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    if !migration.trim().is_empty() {
        sqlx::query(migration).execute(&pool).await.unwrap();
    }
    pool
}

/// Run a raw INSERT (or any statement) against a pool.
pub async fn seed(pool: &SqlitePool, sql: &str) {
    sqlx::query(sql).execute(pool).await.unwrap();
}
