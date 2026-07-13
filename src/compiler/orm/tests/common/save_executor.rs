//! A mock runtime executor for [SavePlan].

use orm::query::save::plan::{PathSegment, SaveArg, SavePlan, SaveQuery, SaveStep, SqlStatement};
use orm::query::{Database, DatabaseKind, TemplateSegment};
use serde_json::{Value, json};

use crate::common::row_to_json;
use crate::common::setup::MockStorage;

/// Executes a save plan against a mutable [MockStorage].
///
/// A save receives no runtime params — every value is carried in the payload the plan was
/// built from — so the executor takes none.
///
/// - Any missing value is a hard failure
/// - KV metadata is ignored for storage
/// - No partial save failures.
pub async fn execute(plan: &SavePlan<'_>, storage: &mut MockStorage) -> Value {
    let mut ctx = Context {
        storage,
        body: Value::Null,
    };

    for stage in &plan.stages {
        for step in &stage.steps {
            ctx.step(step).await;
        }
    }

    ctx.body
}

struct Context<'a> {
    storage: &'a mut MockStorage,
    body: Value,
}

impl<'a> Context<'a> {
    async fn step(&mut self, step: &SaveStep<'_>) {
        match &step.query {
            SaveQuery::SqlBatch {
                database,
                statements,
                shard,
            } => self.sql_batch(database, statements, shard).await,
            SaveQuery::KeyWrite {
                database,
                key: segments,
                value,
                metadata,
                shard,
            } => self.key_write(
                &step.result,
                database,
                segments,
                value,
                metadata.as_deref(),
                shard,
            ),
            SaveQuery::Synthesize { fields, create } => {
                self.synthesize(&step.result, fields, *create)
            }
        }
    }

    async fn sql_batch(
        &mut self,
        database: &Database<'_>,
        statements: &[SqlStatement<'_>],
        shard: &[(&str, SaveArg<'_>)],
    ) {
        // Resolve the pool up front (cloned handle so we don't hold a borrow of storage
        // while mutating the body). A DO batch also tags each hydrated row with its shard
        // values, since the shard fields are route fields not returned by the SELECT.
        let (pool, shard_tags) = match &database.kind {
            DatabaseKind::D1 => (
                self.storage
                    .d1
                    .get(database.name)
                    .expect("D1 binding to exist in tests")
                    .clone(),
                Vec::new(),
            ),
            DatabaseKind::DurableObject => {
                let tags = shard
                    .iter()
                    .map(|(f, arg)| (f.to_string(), self.resolve(arg)))
                    .collect::<Vec<_>>();
                let tuple = tags.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>();
                (
                    self.storage
                        .durable_pool(database.name, &tuple)
                        .await
                        .clone(),
                    tags,
                )
            }
            other => panic!("unsupported save database {other:?}"),
        };

        let mut tx = pool.begin().await.expect("begin");
        for statement in statements {
            match statement {
                SqlStatement::Write { sql, arguments } => {
                    self.bind(sql, arguments)
                        .execute(&mut *tx)
                        .await
                        .expect("write to succeed in tests");
                }
                SqlStatement::Hydrate {
                    sql,
                    arguments,
                    result,
                } => {
                    let mut row = row_to_json(
                        &self
                            .bind(sql, arguments)
                            .fetch_one(&mut *tx)
                            .await
                            .expect("hydrate to return a row in tests"),
                    );
                    if let Value::Object(map) = &mut row {
                        for (field, value) in &shard_tags {
                            map.insert(field.clone(), value.clone());
                        }
                    }
                    self.attach(result, row);
                }
            }
        }
        tx.commit().await.expect("commit");
    }

    fn key_write(
        &mut self,
        result: &[PathSegment<'_>],
        database: &Database<'_>,
        segments: &[TemplateSegment<'_, SaveArg<'_>>],
        value: &Value,
        _metadata: Option<&Value>,
        shard: &[(&str, SaveArg<'_>)],
    ) {
        let key = resolve_key(segments, &self.body);
        let written = value.clone();

        match &database.kind {
            DatabaseKind::R2 => {
                self.storage
                    .r2
                    .entry(database.name.to_string())
                    .or_default()
                    .insert(key, written.clone());
            }
            DatabaseKind::Kv => {
                self.storage
                    .kv
                    .entry(database.name.to_string())
                    .or_default()
                    .insert(key, written.clone());
            }
            DatabaseKind::DurableObject => {
                let tuple = shard
                    .iter()
                    .map(|(_, arg)| resolve_arg(arg, &self.body))
                    .collect::<Vec<_>>();
                self.storage
                    .durable_kv
                    .entry(database.name.to_string())
                    .or_default()
                    .entry(tuple)
                    .or_default()
                    .insert(key, written.clone());
            }
            other => unreachable!("key write routed at non-storage kind {other:?}"),
        }

        self.attach(result, written);
    }

    fn synthesize(
        &mut self,
        result: &[PathSegment<'_>],
        fields: &[(&str, SaveArg<'_>)],
        create: bool,
    ) {
        if create {
            let object = self.build_fields(fields);
            self.attach(result, object);
            return;
        }

        // Merge onto the existing object at `result`.
        if self.body_at(result).is_none() {
            return;
        }
        let additions = self.build_fields(fields);
        if let (Some(Value::Object(map)), Value::Object(add)) =
            (self.body_at_mut(result), additions)
        {
            map.extend(add);
        }
    }

    fn build_fields(&self, fields: &[(&str, SaveArg<'_>)]) -> Value {
        Value::Object(
            fields
                .iter()
                .map(|(field, arg)| (field.to_string(), self.resolve(arg)))
                .collect(),
        )
    }

    fn resolve(&self, arg: &SaveArg<'_>) -> Value {
        match arg {
            SaveArg::Payload(v) => (*v).clone(),
            SaveArg::Result(path) => self
                .body_at(path)
                .cloned()
                .expect("Body value to exist in tests"),
        }
    }

    /// Bind a statement's `?N` slots from its [SaveArg]s. `Value::Null` binds a typed
    /// `None`, bools bind as `i64`, matching the runtime's typed-null handling.
    fn bind<'q>(
        &self,
        sql: &'q str,
        arguments: &[SaveArg<'_>],
    ) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
        arguments
            .iter()
            .fold(sqlx::query(sql), |q, arg| match self.resolve(arg) {
                Value::Null => q.bind(None::<String>),
                Value::Bool(b) => q.bind(b as i64),
                Value::Number(n) if n.is_i64() => q.bind(n.as_i64().unwrap()),
                Value::Number(n) => q.bind(n.as_f64().unwrap()),
                Value::String(s) => q.bind(s),
                other => q.bind(other.to_string()),
            })
    }

    /// The value at an exact [PathSeg] path of the hydrated body.
    fn body_at(&self, path: &[PathSegment<'_>]) -> Option<&Value> {
        path.iter().try_fold(&self.body, |cur, seg| match seg {
            PathSegment::Field(f) => cur.get(*f),
            PathSegment::Index(i) => cur.get(*i),
        })
    }

    fn body_at_mut(&mut self, path: &[PathSegment<'_>]) -> Option<&mut Value> {
        path.iter().try_fold(&mut self.body, |cur, seg| match seg {
            PathSegment::Field(f) => cur.get_mut(*f),
            PathSegment::Index(i) => cur.get_mut(*i),
        })
    }

    /// Attach `value` at an exact [PathSeg] path, creating intermediate objects/arrays
    /// on demand. An empty path replaces the whole body.
    fn attach(&mut self, path: &[PathSegment<'_>], value: Value) {
        let Some((last, parents)) = path.split_last() else {
            // Root attach: merge onto an existing root object (a child hydrated earlier may
            // already sit there) rather than replacing it.
            match (&mut self.body, value) {
                (Value::Object(existing), Value::Object(add)) => existing.extend(add),
                (body, value) => *body = value,
            }
            return;
        };

        let mut cur = &mut self.body;
        for (seg, next) in parents.iter().zip(path.iter().skip(1)) {
            cur = descend(cur, seg, matches!(next, PathSegment::Index(_)));
        }
        place(cur, last, value);
    }
}

/// Step into (creating if needed) the child at `seg`. `next_is_index` decides whether a
/// freshly-created container is an array or an object.
fn descend<'b>(cur: &'b mut Value, seg: &PathSegment<'_>, next_is_index: bool) -> &'b mut Value {
    let empty = || if next_is_index { json!([]) } else { json!({}) };
    match seg {
        PathSegment::Field(f) => {
            if !cur.is_object() {
                *cur = json!({});
            }
            let map = cur.as_object_mut().unwrap();
            map.entry(f.to_string()).or_insert_with(empty)
        }
        PathSegment::Index(i) => {
            if !cur.is_array() {
                *cur = json!([]);
            }
            let arr = cur.as_array_mut().unwrap();
            while arr.len() <= *i {
                arr.push(empty());
            }
            &mut arr[*i]
        }
    }
}

/// Place `value` at the final `seg` of `cur`.
fn place(cur: &mut Value, seg: &PathSegment<'_>, value: Value) {
    match seg {
        PathSegment::Field(f) => {
            if !cur.is_object() {
                *cur = json!({});
            }
            cur.as_object_mut().unwrap().insert(f.to_string(), value);
        }
        PathSegment::Index(i) => {
            if !cur.is_array() {
                *cur = json!([]);
            }
            let arr = cur.as_array_mut().unwrap();
            while arr.len() <= *i {
                arr.push(Value::Null);
            }
            arr[*i] = value;
        }
    }
}

fn resolve_key(segments: &[TemplateSegment<'_, SaveArg<'_>>], body: &Value) -> String {
    let mut out = String::new();
    for segment in segments {
        match segment {
            TemplateSegment::Literal(text) => out.push_str(text),
            TemplateSegment::Value(arg) => match resolve_arg(arg, body) {
                Value::String(s) => out.push_str(&s),
                other => out.push_str(&other.to_string()),
            },
        }
    }
    out
}

fn resolve_arg(arg: &SaveArg<'_>, body: &Value) -> Value {
    match arg {
        SaveArg::Payload(value) => (*value).clone(),
        SaveArg::Result(path) => path
            .iter()
            .try_fold(body, |cur, seg| match seg {
                PathSegment::Field(f) => cur.get(*f),
                PathSegment::Index(i) => cur.get(*i),
            })
            .cloned()
            .expect("Value to exist in tests"),
    }
}
