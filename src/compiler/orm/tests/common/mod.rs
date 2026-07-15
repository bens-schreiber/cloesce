use serde_json::Value;
use sqlx::{Column, Row};

#[allow(dead_code)]
pub mod save_executor;

#[allow(dead_code)]
pub mod select_executor;

#[allow(dead_code)]
pub mod setup;

/// Merge `value` onto `target`: object-into-object extends keys, anything else replaces.
fn merge(target: &mut Value, value: Value) {
    match (target, value) {
        (Value::Object(map), Value::Object(add)) => map.extend(add),
        (target, value) => *target = value,
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

/// Convert a [sqlx::sqlite::SqliteRow] into a JSON object, with column names as keys and
/// values as JSON values.
fn row_to_json(row: &sqlx::sqlite::SqliteRow) -> Value {
    Value::Object(
        row.columns()
            .iter()
            .map(|c| {
                let i = c.ordinal();
                let value = row
                    .try_get::<i64, _>(i)
                    .map(Value::from)
                    .or_else(|_| row.try_get::<f64, _>(i).map(Value::from))
                    .or_else(|_| row.try_get::<String, _>(i).map(Value::from))
                    .expect("Column type to be supported in tests");

                (c.name().to_string(), value)
            })
            .collect(),
    )
}
