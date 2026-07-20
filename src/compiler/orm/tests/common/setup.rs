use std::collections::HashMap;

use idl::{BackingKind, CloesceIdl, IncludeTree, Model};
use migrations::{MigrationsGenerator, MigrationsIdl, MigrationsModel};
use serde_json::Value;
use sqlx::SqlitePool;

pub fn tree(value: Value) -> IncludeTree<'static> {
    let s = serde_json::to_string(&value).unwrap();
    serde_json::from_str(Box::leak(s.into_boxed_str())).unwrap()
}

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

async fn new_pool(migration: &str) -> SqlitePool {
    let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
    if !migration.trim().is_empty() {
        sqlx::query(migration).execute(&pool).await.unwrap();
    }
    pool
}

/// Mock Cloudflare Workers runtime storage for a query plan.
#[derive(Default)]
pub struct MockStorage {
    /// D1 binding name -> pool.
    pub d1: HashMap<String, SqlitePool>,

    /// DO binding name -> (shard value tuple -> pool); each DO instance is its own db.
    pub durable: HashMap<String, HashMap<Vec<Value>, SqlitePool>>,

    /// R2 binding name -> (key -> stored JSON value). A mock simplification of an
    /// `R2ObjectBody`: reads return the raw stored value.
    pub r2: HashMap<String, HashMap<String, Value>>,

    /// Workers KV binding name -> (key -> value).
    pub kv: HashMap<String, HashMap<String, Value>>,

    /// DO-KV: binding name -> (shard value tuple -> (key -> value)). DO storage exists
    /// implicitly, so a shard with no seeded entries is a valid, empty store.
    pub durable_kv: HashMap<String, HashMap<Vec<Value>, HashMap<String, Value>>>,

    /// DO binding name -> its schema migration, so a save to a brand-new shard can create
    /// that stub's pool lazily.
    pub durable_migrations: HashMap<String, String>,
}

impl MockStorage {
    /// Return the pool for a DO stub, creating it (and running its migration) if a test
    /// saves to a shard that was not pre-declared.
    pub async fn durable_pool(&mut self, binding: &str, shard: &[Value]) -> &SqlitePool {
        let migration = self
            .durable_migrations
            .get(binding)
            .cloned()
            .unwrap_or_default();
        let instances = self.durable.entry(binding.to_string()).or_default();
        if !instances.contains_key(shard) {
            instances.insert(shard.to_vec(), new_pool(&migration).await);
        }
        instances.get(shard).unwrap()
    }
}

impl MockStorage {
    pub async fn from_idl(
        idl: &CloesceIdl<'_>,
        shard_inits: &[(&'static str, Vec<Vec<Value>>)],
    ) -> Self {
        let mut by_binding = HashMap::new();
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
        let mut durable = HashMap::new();
        let mut durable_migrations = HashMap::new();
        let shards = shard_inits.iter().cloned().collect::<HashMap<_, _>>();

        for (binding, (kind, models)) in by_binding {
            let migration = migration_for(&models, idl.hash);
            match kind {
                BackingKind::D1 => {
                    let pool = new_pool(&migration).await;
                    d1.insert(binding.to_string(), pool);
                }
                BackingKind::DurableObject => {
                    // A shared schema may declare DO bindings a given test never touches;
                    // those get no shard spec and simply start with no instances.
                    let tuples = shards.get(binding).cloned().unwrap_or_default();
                    let mut instances = HashMap::new();
                    for tuple in tuples {
                        instances.insert(tuple.clone(), new_pool(&migration).await);
                    }
                    durable.insert(binding.to_string(), instances);
                    durable_migrations.insert(binding.to_string(), migration);
                }
            }
        }

        MockStorage {
            d1,
            durable,
            durable_migrations,
            ..Default::default()
        }
    }
}
