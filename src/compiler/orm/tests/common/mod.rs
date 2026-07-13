use serde_json::Value;
use sqlx::{Column, Row};

#[allow(dead_code)]
pub mod save_executor;

#[allow(dead_code)]
pub mod select_executor;

#[allow(dead_code)]
pub mod setup;

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
