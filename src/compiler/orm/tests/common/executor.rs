//! A mock runtime executor for [QueryPlan].
//!
//! Executes the query plan exactly as written, with no access to the IDL, against
//! a set of in-memory SQLite pools.
//!
//! Does not chunk large spreads.

use std::collections::HashMap;

use orm::query::plan::{
    Argument, Cardinality, DatabaseKind, JoinKeys, ObjectPath, Query, QueryPlan, Step,
};
use serde_json::{Map, Value};
use sqlx::{Column, Row, SqlitePool, ValueRef};

/// The backing pools a plan executes against.
pub struct Backends {
    /// D1 binding name -> pool.
    pub d1: HashMap<String, SqlitePool>,

    /// DO binding name -> (shard value tuple -> pool); each DO instance is its own db.
    pub durable: HashMap<String, HashMap<Vec<Value>, SqlitePool>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExecError {
    /// An [Argument::Param] named a runtime parameter that was not supplied.
    MissingParam(String),

    /// A step depended (transitively) on the result of a step that had already failed.
    SkippedDependency(String),

    /// The step could not run: missing backend, unknown shard, or SQL failure.
    Sql(String),
}

/// Execute `plan`, returning the hydrated body and any step failures.
pub async fn execute(
    plan: &QueryPlan<'_>,
    params: Map<String, Value>,
    backends: &Backends,
) -> (Value, Vec<ExecError>) {
    let mut body = Value::Null;
    let mut errors = Vec::new();
    // Result paths whose producing step failed; any step reading through one is skipped.
    let mut failed: Vec<&[&str]> = Vec::new();

    for stage in &plan.stages {
        // Steps within a stage cannot observe each other's output, so all bindings
        // resolve against `body` as it stood at the start of the stage.
        let snapshot = body.clone();
        for step in &stage.steps {
            let result = path_of(&step.result);
            let outcome = if binding_paths(step).any(|p| failed.iter().any(|f| p.starts_with(f))) {
                Err(ExecError::SkippedDependency(result.join(".")))
            } else {
                run_step(step, &params, &snapshot, backends).await
            };
            match outcome {
                Ok(rows) => attach(&mut body, step, rows),
                Err(e) => {
                    failed.push(result);
                    errors.push(e);
                }
            }
        }
    }

    (body, errors)
}

fn path_of<'a>(path: &'a ObjectPath<'a>) -> &'a [&'a str] {
    match path {
        ObjectPath::Root => &[],
        ObjectPath::Field(fields) => fields,
    }
}

/// The result paths this step reads through, across SQL and shard bindings.
fn binding_paths<'a>(step: &'a Step<'a>) -> impl Iterator<Item = &'a [&'a str]> {
    let shard = match &step.database.kind {
        DatabaseKind::DurableObject { shard } => shard.as_slice(),
        _ => &[],
    };
    step.arguments.iter().chain(shard).filter_map(|b| match b {
        Argument::Scalar(path) | Argument::Spread(path) => Some(path_of(path)),
        Argument::Param(_) => None,
    })
}

/// Run a single step's SQL, fanning a Durable Object step out over its shard tuples and
/// tagging each row with its shard values (under the shard join pairs' child keys) so
/// joining stays uniform.
async fn run_step(
    step: &Step<'_>,
    params: &Map<String, Value>,
    body: &Value,
    backends: &Backends,
) -> Result<Vec<Value>, ExecError> {
    let Query::Sql { sql } = &step.query else {
        return Err(ExecError::Sql("non-sql query in mock executor".into()));
    };
    let name = step.database.name;

    match &step.database.kind {
        DatabaseKind::D1 => {
            let pool = backends
                .d1
                .get(name)
                .ok_or_else(|| ExecError::Sql(format!("no d1 pool {name}")))?;
            run_sql(sql, &step.arguments, params, body, pool).await
        }
        DatabaseKind::DurableObject { shard } => {
            let pools = backends
                .durable
                .get(name)
                .ok_or_else(|| ExecError::Sql(format!("no durable binding {name}")))?;

            // Shard join pairs come first in `mapping.join`, in shard-binding order.
            let tag_fields = step
                .mapping
                .join
                .iter()
                .take(shard.len())
                .map(|s| s.child_key)
                .collect::<Vec<_>>();

            let mut rows = Vec::new();
            for tuple in shard_tuples(shard, params, body)? {
                let pool = pools
                    .get(&tuple)
                    .ok_or_else(|| ExecError::Sql(format!("unknown shard {tuple:?} for {name}")))?;
                for mut row in run_sql(sql, &step.arguments, params, body, pool).await? {
                    if let Value::Object(map) = &mut row {
                        map.extend(
                            tag_fields
                                .iter()
                                .zip(&tuple)
                                .map(|(f, v)| (f.to_string(), v.clone())),
                        );
                    }
                    rows.push(row);
                }
            }
            Ok(rows)
        }
        other => Err(ExecError::Sql(format!("unsupported database {other:?}"))),
    }
}

/// The distinct shard tuples to fan a durable step over: each shard binding contributes
/// one column of values, zipped positionally into tuples.
fn shard_tuples(
    shard: &[Argument<'_>],
    params: &Map<String, Value>,
    body: &Value,
) -> Result<Vec<Vec<Value>>, ExecError> {
    let columns = shard
        .iter()
        .map(|b| resolve(b, params, body))
        .collect::<Result<Vec<_>, _>>()?;
    let len = columns.iter().map(|c| c.len()).max().unwrap_or(0);
    Ok(dedup(
        (0..len)
            .map(|i| {
                columns
                    .iter()
                    .map(|c| c.get(i).cloned().unwrap_or(Value::Null))
                    .collect()
            })
            .collect(),
    ))
}

/// Execute `sql` against `pool`: resolve each `?N` slot from `bindings[N-1]`, expand
/// every spread fully, and return the rows.
///
/// We would chunk large spreads here in the  actual runtime
/// (to stay under the backend's bind-parameter limit)
async fn run_sql(
    sql: &str,
    bindings: &[Argument<'_>],
    params: &Map<String, Value>,
    body: &Value,
    pool: &SqlitePool,
) -> Result<Vec<Value>, ExecError> {
    let slots = bindings
        .iter()
        .map(|b| resolve(b, params, body))
        .collect::<Result<Vec<_>, _>>()?;

    // An empty `IN ()` matches nothing, so a spread with no values short-circuits.
    if slots.iter().any(|s| s.is_empty()) {
        return Ok(Vec::new());
    }

    query_rows(&expand(sql, &slots), &slots.concat(), pool).await
}

/// Resolve a binding to the value(s) occupying its `?N` slot: a `Param` is single-valued,
/// a path yields every value at that path across the body (deduped, nulls dropped).
fn resolve(
    binding: &Argument<'_>,
    params: &Map<String, Value>,
    body: &Value,
) -> Result<Vec<Value>, ExecError> {
    match binding {
        Argument::Param(name) => params
            .get(*name)
            .cloned()
            .map(|v| vec![v])
            .ok_or_else(|| ExecError::MissingParam(name.to_string())),
        Argument::Scalar(path) | Argument::Spread(path) => Ok(dedup(collect_at(body, path))),
    }
}

fn dedup<T: PartialEq>(values: Vec<T>) -> Vec<T> {
    values.into_iter().fold(Vec::new(), |mut out, v| {
        if !out.contains(&v) {
            out.push(v);
        }
        out
    })
}

/// Replace each `?N` with one plain `?` per value in `slots[N-1]`. Placeholders appear
/// in ascending order in planner SQL, so anonymous `?`s bind in flattened slot order.
/// Highest index first, so replacing `?1` never clobbers the prefix of `?10`.
fn expand(sql: &str, slots: &[Vec<Value>]) -> String {
    slots
        .iter()
        .enumerate()
        .rev()
        .fold(sql.to_string(), |sql, (i, values)| {
            sql.replace(&format!("?{}", i + 1), &vec!["?"; values.len()].join(", "))
        })
}

async fn query_rows(
    sql: &str,
    binds: &[Value],
    pool: &SqlitePool,
) -> Result<Vec<Value>, ExecError> {
    let query = binds.iter().fold(sqlx::query(sql), |q, v| match v {
        Value::Null => q.bind(None::<String>),
        Value::Bool(b) => q.bind(*b as i64),
        Value::Number(n) if n.is_i64() => q.bind(n.as_i64().unwrap()),
        Value::Number(n) => q.bind(n.as_f64().unwrap()),
        Value::String(s) => q.bind(s.clone()),
        other => q.bind(other.to_string()),
    });
    let rows = query
        .fetch_all(pool)
        .await
        .map_err(|e| ExecError::Sql(e.to_string()))?;
    Ok(rows.iter().map(row_to_json).collect())
}

/// Convert a raw SQLite row into a JSON object keyed by column name. `try_get`
/// type-checks (but decodes NULL as a zero value, hence the explicit null check),
/// so falling through i64 -> f64 -> String is safe.
fn row_to_json(row: &sqlx::sqlite::SqliteRow) -> Value {
    Value::Object(
        row.columns()
            .iter()
            .map(|c| {
                let i = c.ordinal();
                let value = if row.try_get_raw(i).map(|r| r.is_null()).unwrap_or(true) {
                    Value::Null
                } else {
                    row.try_get::<i64, _>(i)
                        .map(Value::from)
                        .or_else(|_| row.try_get::<f64, _>(i).map(Value::from))
                        .or_else(|_| row.try_get::<String, _>(i).map(Value::from))
                        .unwrap_or(Value::Null)
                };
                (c.name().to_string(), value)
            })
            .collect(),
    )
}

/// Every non-null value at `path` across the hydrated body, flattening through arrays.
fn collect_at(body: &Value, path: &ObjectPath) -> Vec<Value> {
    path_of(path)
        .iter()
        .fold(vec![body], |current, key| {
            current
                .into_iter()
                .flat_map(flatten)
                .filter_map(|v| v.get(*key))
                .collect()
        })
        .into_iter()
        .flat_map(flatten)
        .filter(|v| !v.is_null())
        .cloned()
        .collect()
}

/// Expand arrays into their elements; scalars/objects pass through as a single item.
fn flatten(value: &Value) -> Vec<&Value> {
    match value {
        Value::Array(items) => items.iter().flat_map(flatten).collect(),
        other => vec![other],
    }
}

/// Attach `rows` at `step.result`: a root step's rows become the body itself; a nav
/// step's rows are joined under the final path segment of every parent they match.
fn attach(body: &mut Value, step: &Step<'_>, rows: Vec<Value>) {
    match path_of(&step.result).split_last() {
        None => *body = shape(rows, step.mapping.cardinality),
        Some((field, parents)) => {
            for parent in parents_at_mut(body, parents) {
                let matched = rows
                    .iter()
                    .filter(|row| join(parent, row, &step.mapping.join))
                    .cloned()
                    .collect();
                if let Value::Object(map) = parent {
                    map.insert(field.to_string(), shape(matched, step.mapping.cardinality));
                }
            }
        }
    }
}

fn shape(rows: Vec<Value>, cardinality: Cardinality) -> Value {
    match cardinality {
        Cardinality::One => rows.into_iter().next().unwrap_or(Value::Null),
        Cardinality::Many => Value::Array(rows),
    }
}

/// True if `row` belongs on `parent`: every pair must satisfy
/// `parent[parent_key] == row[child_key]`. An empty join matches every parent.
fn join(parent: &Value, row: &Value, keys: &[JoinKeys]) -> bool {
    keys.iter().all(|pair| {
        matches!(
            (parent.get(pair.parent_key), row.get(pair.child_key)),
            (Some(a), Some(b)) if a == b
        )
    })
}

/// Mutable references to every object reached by following `path` through the body,
/// flattening through arrays.
fn parents_at_mut<'a>(body: &'a mut Value, path: &[&str]) -> Vec<&'a mut Value> {
    path.iter()
        .fold(vec![body], |current, key| {
            current
                .into_iter()
                .flat_map(flatten_mut)
                .filter_map(|v| v.get_mut(*key))
                .collect()
        })
        .into_iter()
        .flat_map(flatten_mut)
        .collect()
}

fn flatten_mut(value: &mut Value) -> Vec<&mut Value> {
    match value {
        Value::Array(items) => items.iter_mut().flat_map(flatten_mut).collect(),
        other => vec![other],
    }
}
