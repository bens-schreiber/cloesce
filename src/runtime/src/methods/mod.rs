pub mod orm;
pub mod upsert;

use common::CidlType;
use sea_query::{Expr, SimpleExpr};
use serde_json::Value;

#[cfg(test)]
#[macro_export]
macro_rules! expected_str {
    ($got:expr, $expected:expr) => {{
        let got_val = &$got;
        let expected_val = &$expected;
        assert!(
            got_val.to_string().contains(&expected_val.to_string()),
            "Expected: \n`{}`, \n\ngot:\n{:?}",
            expected_val,
            got_val
        );
    }};
}

fn alias(name: impl Into<String>) -> sea_query::Alias {
    sea_query::Alias::new(name)
}

/// Validates that a JSON input follows the CIDL type, returning
/// a SeaQuery [SimpleExpr] value
fn validate_json_to_cidl(
    value: &Value,
    cidl_type: &CidlType,
    model_name: &str,
    attr_name: &str,
) -> Result<SimpleExpr, String> {
    if matches!(cidl_type, CidlType::Nullable(_)) && value.is_null() {
        return Ok(SimpleExpr::Custom("null".into()));
    }

    match cidl_type.root_type() {
        CidlType::Integer => {
            if !matches!(value, Value::Number(_)) {
                return Err(format!(
                    "Expected an integer type for {}.{}",
                    model_name, attr_name
                ));
            }

            Ok(Expr::val(value.as_i64().unwrap()).into())
        }
        CidlType::Real => {
            if !matches!(value, Value::Number(_)) {
                return Err(format!(
                    "Expected an real type for {}.{}",
                    model_name, attr_name
                ));
            }

            Ok(Expr::val(value.as_f64().unwrap()).into())
        }
        CidlType::Text | CidlType::Blob => {
            if !matches!(value, Value::String(_)) {
                return Err(format!(
                    "Expected an real type for {}.{}",
                    model_name, attr_name
                ));
            }

            Ok(Expr::val(value.as_str().unwrap()).into())
        }
        _ => {
            unreachable!("Invalid CIDL");
        }
    }
}
