use std::collections::HashMap;

use idl::{BackingKind, CloesceIdl, Model};
use migrations::{MigrationsGenerator, MigrationsIdl, MigrationsModel};
use serde_json::Value;
use sqlx::SqlitePool;

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

    /// Workers KV binding name -> (key -> (value, metadata)).
    pub kv: HashMap<String, HashMap<String, (Value, Option<Value>)>>,

    /// DO-KV: binding name -> (shard value tuple -> (key -> value)). DO storage exists
    /// implicitly, so a shard with no seeded entries is a valid, empty store.
    pub durable_kv: HashMap<String, HashMap<Vec<Value>, HashMap<String, Value>>>,
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
                }
            }
        }

        MockStorage {
            d1,
            durable,
            ..Default::default()
        }
    }

    pub async fn seed_d1(&mut self, binding: &str, sql: &str) {
        let pool = self.d1.get(binding).unwrap();
        sqlx::query(sql).execute(pool).await.unwrap();
    }

    pub async fn seed_do(&mut self, binding: &str, shard: Vec<Value>, sql: &str) {
        let pool = self.durable.get(binding).unwrap().get(&shard).unwrap();
        sqlx::query(sql).execute(pool).await.unwrap();
    }

    pub fn seed_r2(&mut self, binding: &str, key: &str, value: Value) {
        self.r2
            .entry(binding.to_string())
            .or_default()
            .insert(key.to_string(), value);
    }

    pub fn seed_kv(&mut self, binding: &str, key: &str, value: Value, metadata: Option<Value>) {
        self.kv
            .entry(binding.to_string())
            .or_default()
            .insert(key.to_string(), (value, metadata));
    }

    pub fn seed_durable_kv(&mut self, binding: &str, shard: Vec<Value>, key: &str, value: Value) {
        self.durable_kv
            .entry(binding.to_string())
            .or_default()
            .entry(shard)
            .or_default()
            .insert(key.to_string(), value);
    }
}
