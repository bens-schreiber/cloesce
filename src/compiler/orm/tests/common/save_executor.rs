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
/// - Any missing value is a hard failure
/// - KV metadata is ignored for storage
/// - No partial save failures. If any step fails, the whole plan fails. In the real runtime
///   a failed step still lets later steps in its stage attach their results.
pub async fn execute(plan: &SavePlan<'_>, storage: &mut MockStorage) -> Value {
    let mut ctx = Context {
        storage,
        body: Value::Null,
    };

    for stage in &plan.stages {
        // Resolve values from the frozen body
        let mut prepped = Vec::with_capacity(stage.steps.len());
        for step in &stage.steps {
            prepped.push(ctx.prep(step).await);
        }

        // Run all steps in parallel
        let fetched = futures::future::join_all(prepped.into_iter().map(Prepped::fetch)).await;

        // Attach each steps result in step order.
        for (step, fetched) in stage.steps.iter().zip(fetched) {
            ctx.sink(step, fetched);
        }
    }

    ctx.body
}

/// `(result path, value)` pairs a step attaches in the sink, in statement order.
type Attachments<'src> = Vec<(&'src [PathSegment<'src>], Value)>;

/// A single prepared statement carried from prep into the parallel fetch.
struct PreppedStatement<'src> {
    sql: &'src str,

    /// Bind values pre-resolved from the frozen body, in `?N` order.
    binds: Vec<Value>,

    /// `Some(result)` for a Hydrate readback, `None` for a plain write.
    hydrate: Option<&'src [PathSegment<'src>]>,
}

/// A step after prep: everything needing `&mut storage` or a body read is done, leaving
/// only work that can run in parallel (SqlBatch transactions) or at sink time.
enum Prepped<'src> {
    /// A SqlBatch ready to run its transaction in the parallel fetch.
    Batch {
        pool: sqlx::SqlitePool,
        shard_tags: Vec<(String, Value)>,
        statements: Vec<PreppedStatement<'src>>,
    },

    /// Attachments already resolved in prep.
    ///
    /// Empty for a [SaveQuery::Synthesize] step with `create = false`
    /// (merge onto an existing object).
    Ready(Attachments<'src>),
}

impl<'src> Prepped<'src> {
    /// Run the parallel part of a prepped step: a SqlBatch's transaction, yielding its
    /// hydrated rows
    async fn fetch(self) -> Attachments<'src> {
        let (pool, shard_tags, statements) = match self {
            Prepped::Ready(attachments) => return attachments,
            Prepped::Batch {
                pool,
                shard_tags,
                statements,
            } => (pool, shard_tags, statements),
        };

        let mut rows = Vec::new();
        let mut tx = pool.begin().await.expect("begin");
        for stmt in statements {
            let query = stmt
                .binds
                .iter()
                .fold(sqlx::query(stmt.sql), |q, v| bind_value(q, v));
            match stmt.hydrate {
                None => {
                    query
                        .execute(&mut *tx)
                        .await
                        .expect("write to succeed in tests");
                }
                Some(result) => {
                    let mut row = row_to_json(
                        &query
                            .fetch_one(&mut *tx)
                            .await
                            .expect("hydrate to return a row in tests"),
                    );
                    if let Value::Object(map) = &mut row {
                        for (field, value) in &shard_tags {
                            map.insert(field.clone(), value.clone());
                        }
                    }
                    rows.push((result, row));
                }
            }
        }
        tx.commit().await.expect("commit");
        rows
    }
}

struct Context<'a> {
    storage: &'a mut MockStorage,
    body: Value,
}

impl<'a> Context<'a> {
    /// Prepare a step: resolve everything that needs `&mut storage` or reads the frozen
    /// body, so the fetch phase can run in parallel.
    async fn prep<'src>(&mut self, step: &'src SaveStep<'src>) -> Prepped<'src> {
        match &step.query {
            SaveQuery::SqlBatch {
                database,
                statements,
                shard,
            } => self.prep_sql_batch(database, statements, shard).await,
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
            SaveQuery::Synthesize { .. } => Prepped::Ready(vec![]),
        }
    }

    /// Attach a step's fetched result into the body. Runs in step order.
    fn sink(&mut self, step: &SaveStep<'_>, attachments: Attachments<'_>) {
        for (result, value) in attachments {
            self.attach(result, value);
        }
        if let SaveQuery::Synthesize { fields, create } = &step.query {
            self.synthesize(&step.result, fields, *create);
        }
    }

    async fn prep_sql_batch<'src>(
        &mut self,
        database: &Database<'_>,
        statements: &'src [SqlStatement<'src>],
        shard: &[(&str, SaveArg<'_>)],
    ) -> Prepped<'src> {
        // Resolve the pool up front. A DO batch also tags each hydrated row with its shard
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

        // Pre-resolve every statement's bind values from the frozen body so the fetch
        // future borrows nothing of `self`.
        let statements = statements
            .iter()
            .map(|statement| match statement {
                SqlStatement::Write { sql, arguments } => PreppedStatement {
                    sql,
                    binds: arguments.iter().map(|a| self.resolve(a)).collect(),
                    hydrate: None,
                },
                SqlStatement::Hydrate {
                    sql,
                    arguments,
                    result,
                } => PreppedStatement {
                    sql,
                    binds: arguments.iter().map(|a| self.resolve(a)).collect(),
                    hydrate: Some(result),
                },
            })
            .collect();

        Prepped::Batch {
            pool,
            shard_tags,
            statements,
        }
    }

    fn key_write<'src>(
        &mut self,
        result: &'src [PathSegment<'src>],
        database: &Database<'_>,
        segments: &[TemplateSegment<'_, SaveArg<'_>>],
        value: &Value,
        _metadata: Option<&Value>,
        shard: &[(&str, SaveArg<'_>)],
    ) -> Prepped<'src> {
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

        Prepped::Ready(vec![(result, written)])
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

/// Bind one pre-resolved `?N` slot. `Value::Null` binds a typed `None`, bools bind as
/// `i64`, matching the runtime's typed-null handling.
fn bind_value<'q>(
    q: sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>>,
    value: &Value,
) -> sqlx::query::Query<'q, sqlx::Sqlite, sqlx::sqlite::SqliteArguments<'q>> {
    match value {
        Value::Null => q.bind(None::<String>),
        Value::Bool(b) => q.bind(*b as i64),
        Value::Number(n) if n.is_i64() => q.bind(n.as_i64().unwrap()),
        Value::Number(n) => q.bind(n.as_f64().unwrap()),
        Value::String(s) => q.bind(s.clone()),
        other => q.bind(other.to_string()),
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
