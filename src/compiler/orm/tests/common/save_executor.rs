//! A mock runtime executor for [SavePlan].

use orm::query::save::plan::{PathSegment, SaveArg, SavePlan, SaveQuery, SaveStep, SqlStatement};
use orm::query::select::plan::MapCardinality;
use orm::query::{DatabaseKind, TemplateSegment};
use serde_json::{Value, json};

use crate::common::setup::MockStorage;
use crate::common::{bind_value, merge, row_to_json};

/// Executes a save plan against a mutable [MockStorage].
///
/// Some differences from the real runtime:
/// - Any missing value is a hard failure
/// - KV metadata is ignored for storage
/// - No partial save failures. If any step fails, the whole plan fails. In the real runtime
///   a failed step still lets later steps in its stage attach their results.
pub async fn execute(plan: &SavePlan<'_>, storage: &mut MockStorage) -> Value {
    let mut body = Value::Null;

    for stage in &plan.stages {
        let mut handles = Vec::with_capacity(stage.steps.len());
        for step in &stage.steps {
            handles.push(resolve_handle(step, &body, storage).await);
        }

        // The runtime would run each step in parallel, then order the output
        // in step order. This mock just runs them sequentially.
        let mut outs = Vec::with_capacity(stage.steps.len());
        for (step, handle) in stage.steps.iter().zip(handles) {
            outs.push(run_step(step, handle, &body).await);
        }

        // Sequential sink: apply deferred storage writes and attach results, in step order.
        for (step, out) in stage.steps.iter().zip(outs) {
            sink(step, out, &mut body, storage);
        }
    }

    body
}

enum Handle<'src> {
    Sql {
        pool: sqlx::SqlitePool,

        /// For a DO batch, the `(field, value)` shard tags to stamp onto hydrated rows.
        shard_tags: Vec<(String, Value)>,
        statements: &'src [SqlStatement<'src>],
    },
    None,
}

async fn resolve_handle<'src>(
    step: &'src SaveStep<'src>,
    body: &Value,
    storage: &mut MockStorage,
) -> Handle<'src> {
    let SaveQuery::SqlBatch {
        database,
        statements,
        shard,
    } = &step.query
    else {
        return Handle::None;
    };

    let (pool, shard_tags) = match &database.kind {
        DatabaseKind::D1 => (
            storage
                .d1
                .get(database.name)
                .expect("D1 binding to exist in tests")
                .clone(),
            Vec::new(),
        ),
        DatabaseKind::DurableObject => {
            let tags = shard
                .iter()
                .map(|(f, arg)| (f.to_string(), resolve(arg, body)))
                .collect::<Vec<_>>();
            let tuple = tags.iter().map(|(_, v)| v.clone()).collect::<Vec<_>>();
            (
                storage.durable_pool(database.name, &tuple).await.clone(),
                tags,
            )
        }
        other => panic!("unsupported save database {other:?}"),
    };

    Handle::Sql {
        pool,
        shard_tags,
        statements,
    }
}

/// A deferred storage write applied in the sink, once its containing step is due.
struct Write {
    kind: DatabaseKind,
    binding: String,
    key: String,
    value: Value,

    /// The DO shard tuple this write routes to; empty for R2/KV.
    shard: Vec<Value>,
}

enum StepResult<'src> {
    Attach(Vec<(&'src [PathSegment<'src>], Value)>),
    Write(Write, &'src [PathSegment<'src>]),

    /// A [SaveQuery::Synthesize], deferred to the sink to merge onto the existing
    /// hydrated body.
    Synthesize,
}

async fn run_step<'src>(
    step: &'src SaveStep<'src>,
    handle: Handle<'src>,
    body: &Value,
) -> StepResult<'src> {
    match &step.query {
        SaveQuery::SqlBatch { .. } => {
            let Handle::Sql {
                pool,
                shard_tags,
                statements,
            } = handle
            else {
                unreachable!("SqlBatch step always resolves a Sql handle")
            };
            StepResult::Attach(run_sql_batch(&pool, &shard_tags, statements, body).await)
        }
        SaveQuery::KeyWrite {
            database,
            segments,
            value,
            shard,
            ..
        } => StepResult::Write(
            Write {
                kind: database.kind.clone(),
                binding: database.name.to_string(),
                shard: shard.iter().map(|(_, arg)| resolve(arg, body)).collect(),
                key: resolve_key(segments, body),
                value: (*value).clone(),
            },
            &step.result,
        ),
        SaveQuery::Synthesize { .. } => StepResult::Synthesize,
    }
}

/// Run one batch's statements as a single transaction, in a fold over binds resolved from
/// the frozen `body`.
async fn run_sql_batch<'src>(
    pool: &sqlx::SqlitePool,
    shard_tags: &[(String, Value)],
    statements: &'src [SqlStatement<'src>],
    body: &Value,
) -> Vec<(&'src [PathSegment<'src>], Value)> {
    let mut rows = Vec::new();
    let mut tx = pool.begin().await.expect("begin");
    for statement in statements {
        let (sql, arguments) = match statement {
            SqlStatement::Write { sql, arguments } => (sql, arguments),
            SqlStatement::Hydrate { sql, arguments, .. } => (sql, arguments),
        };
        let query = arguments
            .iter()
            .map(|a| resolve(a, body))
            .fold(sqlx::query(sql), |q, v| bind_value(q, &v));

        match statement {
            SqlStatement::Write { .. } => {
                query
                    .execute(&mut *tx)
                    .await
                    .expect("write to succeed in tests");
            }
            SqlStatement::Hydrate { result, .. } => {
                let mut row = row_to_json(
                    &query
                        .fetch_one(&mut *tx)
                        .await
                        .expect("hydrate to return a row in tests"),
                );
                if let Value::Object(map) = &mut row {
                    for (field, value) in shard_tags {
                        map.insert(field.clone(), value.clone());
                    }
                }
                rows.push((result.as_slice(), row));
            }
        }
    }
    tx.commit().await.expect("commit");
    rows
}

/// Apply a [StepResult], mutating `body` and `storage`. Runs in step order.
fn sink(step: &SaveStep, out: StepResult, body: &mut Value, storage: &mut MockStorage) {
    match out {
        StepResult::Attach(attachments) => {
            for (path, value) in attachments {
                attach(body, path, value);
            }
        }
        StepResult::Write(write, result) => {
            let map = match write.kind {
                DatabaseKind::R2 => storage.r2.entry(write.binding).or_default(),
                DatabaseKind::Kv => storage.kv.entry(write.binding).or_default(),
                DatabaseKind::DurableObject => storage
                    .durable_kv
                    .entry(write.binding)
                    .or_default()
                    .entry(write.shard)
                    .or_default(),
                other => unreachable!("key write routed at non-storage kind {other:?}"),
            };
            map.insert(write.key, write.value.clone());
            attach(body, result, write.value);
        }
        StepResult::Synthesize => {
            let SaveQuery::Synthesize {
                fields,
                create,
                cardinality,
            } = &step.query
            else {
                unreachable!("StepOut::Synthesize only produced for a Synthesize step")
            };
            synthesize(body, &step.result, fields, *create, *cardinality);
        }
    }
}

fn synthesize(
    body: &mut Value,
    result: &[PathSegment<'_>],
    fields: &[(&str, SaveArg<'_>)],
    create: bool,
    cardinality: MapCardinality,
) {
    if create {
        let value = match cardinality {
            MapCardinality::One => build_fields(fields, body),
            MapCardinality::Many if fields.is_empty() => json!([]),
            MapCardinality::Many => json!([build_fields(fields, body)]),
        };
        attach(body, result, value);
        return;
    }

    // Merge onto the existing object at `result`; a slot with nothing there is untouched.
    if body_at(body, result).is_none() {
        return;
    }
    let additions = build_fields(fields, body);
    if let (Some(Value::Object(map)), Value::Object(add)) = (body_at_mut(body, result), additions) {
        map.extend(add);
    }
}

fn build_fields(fields: &[(&str, SaveArg<'_>)], body: &Value) -> Value {
    Value::Object(
        fields
            .iter()
            .map(|(field, arg)| (field.to_string(), resolve(arg, body)))
            .collect(),
    )
}

/// Resolve a [SaveArg]: a payload literal, or a value read from `body` at an exact path
/// (a generated PK hydrated by an earlier stage's read-back). Any missing value is a hard
/// failure in tests.
fn resolve(arg: &SaveArg<'_>, body: &Value) -> Value {
    match arg {
        SaveArg::Payload(v) => v.clone().into_owned(),
        SaveArg::Result(path) => body_at(body, path)
            .cloned()
            .expect("Body value to exist in tests"),
    }
}

fn body_at<'b>(body: &'b Value, path: &[PathSegment<'_>]) -> Option<&'b Value> {
    path.iter().try_fold(body, |cur, seg| match seg {
        PathSegment::Field(f) => cur.get(*f),
        PathSegment::Index(i) => cur.get(*i),
    })
}

fn body_at_mut<'b>(body: &'b mut Value, path: &[PathSegment<'_>]) -> Option<&'b mut Value> {
    path.iter().try_fold(body, |cur, seg| match seg {
        PathSegment::Field(f) => cur.get_mut(*f),
        PathSegment::Index(i) => cur.get_mut(*i),
    })
}

/// Attach `value` at an exact [PathSegment] path, creating intermediate objects/arrays on
/// demand. An empty path attaches at the root, merging onto an existing root object (a
/// child hydrated earlier may already sit there) rather than replacing it.
fn attach(body: &mut Value, path: &[PathSegment<'_>], value: Value) {
    let Some((last, parents)) = path.split_last() else {
        merge(body, value);
        return;
    };

    let mut cur = body;
    for (seg, next) in parents.iter().zip(path.iter().skip(1)) {
        cur = descend(cur, seg, matches!(next, PathSegment::Index(_)));
    }
    place(cur, last, value);
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
            cur.as_object_mut()
                .unwrap()
                .entry(f.to_string())
                .or_insert_with(empty)
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
    segments
        .iter()
        .map(|segment| match segment {
            TemplateSegment::Literal(text) => (*text).to_string(),
            TemplateSegment::Value(arg) => match resolve(arg, body) {
                Value::String(s) => s,
                other => other.to_string(),
            },
        })
        .collect()
}
