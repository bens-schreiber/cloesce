use std::collections::HashMap;

use ast::{CidlType, CloesceAst, Number, ValidatedField, Validator};

use base64::{Engine, prelude::BASE64_STANDARD};
use serde_json::Value;

use crate::{OrmErrorKind, fail, fmt_cidl_type};

/// Runtime type validation, asserting that the structure of a JSON value
/// matches the structure of the provided CIDL type.
///
/// Additionally, runs any validators on the value (should it be an [ast::ValidatedField])
pub fn validate_cidl_type(
    field: &ValidatedField,
    value: Option<Value>,
    ast: &CloesceAst,
    partial: bool,
) -> Result<Option<Value>, OrmErrorKind> {
    let cidl_type = &field.cidl_type;

    // Json accepts anything
    if matches!(cidl_type, CidlType::Json) {
        return Ok(value);
    }

    let is_partial = partial || matches!(&cidl_type, CidlType::Partial { .. });

    let Some(mut value) = value else {
        // We will let arrays be undefined and interpret that as an empty array.
        if let CidlType::Array(_) = cidl_type {
            return Ok(Some(Value::Array(vec![])));
        }

        if is_partial {
            return Ok(None);
        }

        fail!(OrmErrorKind::MissingField {
            expected: fmt_cidl_type(cidl_type),
            missing: field.name.to_string(),
        });
    };

    let is_nullable = matches!(&cidl_type, CidlType::Nullable(_));
    if value.is_null() || value == Value::String("null".to_string()) {
        // NOTE: Partial types are always nullable.
        if is_nullable || is_partial {
            return Ok(Some(Value::Null));
        }

        fail!(OrmErrorKind::MissingField {
            expected: fmt_cidl_type(cidl_type),
            missing: field.name.to_string(),
        });
    }

    let unwrapped_type = match cidl_type {
        CidlType::Nullable(inner) => inner,
        _ => cidl_type,
    };

    let type_mismatch_err = |value| OrmErrorKind::TypeMismatch {
        expected: fmt_cidl_type(unwrapped_type),
        got: value,
    };

    let result = match unwrapped_type {
        CidlType::Int => match &value {
            Value::Number(num) if num.is_i64() => Some(value),
            Value::String(s) if s.parse::<i64>().is_ok() => {
                value = Value::Number(s.parse::<i64>().unwrap().into());
                Some(value)
            }
            _ => fail!(type_mismatch_err(value)),
        },
        CidlType::Uint => match &value {
            Value::Number(num) if num.is_u64() => Some(value),
            Value::String(s) if s.parse::<u64>().is_ok() => {
                value = Value::Number(s.parse::<u64>().unwrap().into());
                Some(value)
            }
            _ => fail!(type_mismatch_err(value)),
        },
        CidlType::Real => match &value {
            Value::Number(num) if num.is_f64() || num.is_i64() => Some(value),
            Value::String(s) if s.parse::<f64>().is_ok() => {
                value =
                    Value::Number(serde_json::Number::from_f64(s.parse::<f64>().unwrap()).unwrap());
                Some(value)
            }
            _ => fail!(type_mismatch_err(value)),
        },

        CidlType::String => {
            if value.is_string() {
                Some(value)
            } else {
                fail!(type_mismatch_err(value))
            }
        }

        CidlType::Boolean => match &value {
            Value::Bool(_) => Some(value),
            Value::String(s) if s.eq_ignore_ascii_case("true") => Some(Value::Bool(true)),
            Value::String(s) if s.eq_ignore_ascii_case("false") => Some(Value::Bool(false)),
            _ => fail!(type_mismatch_err(value)),
        },

        CidlType::DateIso => {
            let valid = value
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .is_some();
            if valid {
                Some(value)
            } else {
                fail!(type_mismatch_err(value))
            }
        }

        CidlType::Blob => {
            if let Value::String(s) = &value {
                match BASE64_STANDARD.decode(s) {
                    Ok(bytes) => Some(Value::Array(
                        bytes.into_iter().map(|b| Value::Number(b.into())).collect(),
                    )),
                    Err(_) => fail!(type_mismatch_err(value)),
                }
            } else if let Value::Array(arr) = &value {
                if arr.iter().any(|v| !v.is_u64() || v.as_u64().unwrap() > 255) {
                    fail!(type_mismatch_err(value));
                }
                Some(value)
            } else {
                fail!(type_mismatch_err(value))
            }
        }

        CidlType::R2Object => {
            #[allow(dead_code)]
            #[derive(serde::Deserialize)]
            struct R2Object {
                key: String,
                version: String,
                size: i64,
                etag: String,
                http_etag: String,
                uploaded: chrono::DateTime<chrono::Utc>,
                custom_metadata: Option<HashMap<String, String>>,
            }

            let valid = value
                .as_object()
                .and_then(|obj| serde_json::from_value::<R2Object>(Value::Object(obj.clone())).ok())
                .is_some();
            if valid {
                Some(value)
            } else {
                fail!(type_mismatch_err(value))
            }
        }

        CidlType::DataSource { model_name } => {
            let model = ast.models.get(model_name).unwrap();
            let Some(value_str) = value.as_str() else {
                fail!(type_mismatch_err(value));
            };

            if !model.data_sources.contains_key(value_str) {
                fail!(type_mismatch_err(value));
            }

            Some(value)
        }

        CidlType::KvObject(inner) => {
            if !value.is_object() {
                fail!(type_mismatch_err(value));
            }
            let obj = value.as_object_mut().unwrap();
            let key = obj.remove("key");
            let raw = obj.remove("raw");
            let metadata = obj.remove("metadata");

            let mut new_obj = serde_json::Map::<String, Value>::new();

            // Key must exist and be a string
            if !partial && !matches!(key, Some(Value::String(_))) {
                fail!(OrmErrorKind::MissingField {
                    expected: fmt_cidl_type(&CidlType::String),
                    missing: "key".to_string(),
                })
            }
            new_obj.insert("key".to_string(), key.unwrap_or(Value::Null));

            // Metadata must be an object or null if it exists
            if let Some(metadata) = metadata.to_owned()
                && !(metadata.is_object() || metadata.is_null())
                && !partial
            {
                fail!(OrmErrorKind::TypeMismatch {
                    expected: fmt_cidl_type(&CidlType::Json),
                    got: metadata
                })
            }
            new_obj.insert("metadata".to_string(), metadata.unwrap_or(Value::Null));

            // Validators apply to the inner type
            let raw = validate_cidl_type(
                &ValidatedField {
                    name: "raw".into(),
                    cidl_type: *inner.clone(),
                    validators: field.validators.clone(),
                },
                raw,
                ast,
                partial,
            )?;
            if let Some(raw) = raw {
                new_obj.insert("raw".to_string(), raw);
            }

            return Ok(Some(Value::Object(new_obj)));
        }

        // Plain old objects
        CidlType::Object { name } | CidlType::Partial { object_name: name }
            if let Some(poo) = ast.poos.get(name) =>
        {
            if !value.is_object() {
                fail!(type_mismatch_err(value));
            }
            let obj = value.as_object_mut().unwrap();
            let mut new_obj = serde_json::Map::<String, Value>::new();

            for field in &poo.fields {
                let field_value = obj.remove(field.name.as_ref());
                let res = validate_cidl_type(
                    field,
                    field_value,
                    ast,
                    is_partial || matches!(cidl_type, CidlType::Partial { .. }),
                )?;

                if let Some(res) = res {
                    new_obj.insert(field.name.to_string(), res);
                }
            }

            Some(Value::Object(new_obj))
        }

        // Models
        CidlType::Object { name } | CidlType::Partial { object_name: name } => {
            let mut new_obj = serde_json::Map::<String, Value>::new();
            if !value.is_object() {
                fail!(type_mismatch_err(value));
            }
            let obj = value.as_object_mut().unwrap();
            let model = ast.models.get(name).unwrap();

            for field in &model.key_fields {
                let field_value = obj.remove(field.name.as_ref());
                let res = validate_cidl_type(field, field_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(field.name.to_string(), res);
                }
            }

            for (col, _) in model.all_columns() {
                let col_value = obj.remove(col.field.name.as_ref());
                let res = validate_cidl_type(&col.field, col_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(col.field.name.to_string(), res);
                }
            }

            for nav in &model.navigation_fields {
                let nav_value = obj.remove(nav.field.name.as_ref());
                if nav_value.is_none() {
                    // Does not need to exist.
                    continue;
                }

                let res = validate_cidl_type(
                    &ValidatedField {
                        name: nav.field.name.as_ref().into(),
                        cidl_type: nav.field.cidl_type.clone(),
                        validators: vec![],
                    },
                    nav_value,
                    ast,
                    is_partial,
                )?;

                if let Some(res) = res {
                    new_obj.insert(nav.field.name.to_string(), res);
                }
            }

            for kv_field in &model.kv_fields {
                let kv_field_value = obj.remove(kv_field.field.name.as_ref());
                if kv_field_value.is_none() {
                    // Does not need to exist.
                    continue;
                }

                let res = validate_cidl_type(&kv_field.field, kv_field_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(kv_field.field.name.to_string(), res);
                }
            }

            for r2_obj_meta in &model.r2_fields {
                let r2_obj_value = obj.remove(r2_obj_meta.field.name.as_ref());
                if r2_obj_value.is_none() {
                    // Does not need to exist.
                    continue;
                }
                let res = validate_cidl_type(
                    &ValidatedField {
                        name: r2_obj_meta.field.name.as_ref().into(),
                        cidl_type: CidlType::R2Object,
                        validators: vec![],
                    },
                    r2_obj_value,
                    ast,
                    is_partial,
                )?;

                if let Some(res) = res {
                    new_obj.insert(r2_obj_meta.field.name.to_string(), res);
                }
            }

            Some(Value::Object(new_obj))
        }

        CidlType::Array(cidl_type) => {
            if !value.is_array() {
                fail!(type_mismatch_err(value));
            }
            let arr = value.as_array().unwrap();
            let mut new_arr = Vec::<Value>::new();
            let field = ValidatedField {
                name: field.name.clone(),
                cidl_type: *cidl_type.clone(),
                validators: field.validators.clone(),
            };
            for item in arr {
                let res = validate_cidl_type(&field, Some(item.clone()), ast, is_partial)?;
                if let Some(res) = res {
                    new_arr.push(res);
                }
            }
            Some(Value::Array(new_arr))
        }

        CidlType::Paginated(inner) => {
            if !value.is_object() {
                fail!(type_mismatch_err(value));
            }
            let obj = value.as_object_mut().unwrap();
            let mut new_obj = serde_json::Map::<String, Value>::new();

            // Validate results array
            let results = obj.remove("results");

            let results_value = validate_cidl_type(
                &ValidatedField {
                    name: "results".into(),
                    cidl_type: CidlType::Array(inner.clone()),
                    validators: vec![],
                },
                results,
                ast,
                is_partial,
            )?;
            if let Some(results_value) = results_value {
                new_obj.insert("results".to_string(), results_value);
            }

            // Validate cursor (string | null)
            let cursor = obj.remove("cursor");
            if let Some(cursor_value) = cursor {
                if !cursor_value.is_string() && !cursor_value.is_null() {
                    fail!(type_mismatch_err(cursor_value));
                }
                new_obj.insert("cursor".to_string(), cursor_value);
            } else {
                new_obj.insert("cursor".to_string(), Value::Null);
            }

            // Validate complete (boolean)
            let complete = obj.remove("complete");
            let complete_value = validate_cidl_type(
                &ValidatedField {
                    name: "complete".into(),
                    cidl_type: CidlType::Boolean,
                    validators: vec![],
                },
                complete,
                ast,
                is_partial,
            )?;
            if let Some(complete_value) = complete_value {
                new_obj.insert("complete".to_string(), complete_value);
            }

            Some(Value::Object(new_obj))
        }

        _ => unimplemented!(),
    };

    if let Some(v) = &result {
        // Validators are only ran on a defined, non-null value.
        run_validators(v, &field.validators)?;
    }

    Ok(result)
}

fn run_validators(value: &Value, validators: &[Validator]) -> Result<(), OrmErrorKind> {
    for v in validators {
        match v {
            Validator::GreaterThan(number) => match number {
                Number::Int(i) => {
                    let value_num = value.as_i64().expect("type validation to have run");
                    if value_num <= *i {
                        fail!(OrmErrorKind::NotGreaterThan {
                            expected: Number::Int(*i),
                            got: value.clone(),
                        });
                    }
                }
                Number::Float(f) => {
                    let value_num = value.as_f64().expect("type validation to have run");
                    if value_num <= *f {
                        fail!(OrmErrorKind::NotGreaterThan {
                            expected: Number::Float(*f),
                            got: value.clone(),
                        });
                    }
                }
            },
            Validator::GreaterThanOrEqual(number) => match number {
                Number::Int(i) => {
                    let value_num = value.as_i64().expect("type validation to have run");
                    if value_num < *i {
                        fail!(OrmErrorKind::NotGreaterThanOrEqual {
                            expected: Number::Int(*i),
                            got: value.clone(),
                        });
                    }
                }
                Number::Float(f) => {
                    let value_num = value.as_f64().expect("type validation to have run");
                    if value_num < *f {
                        fail!(OrmErrorKind::NotGreaterThanOrEqual {
                            expected: Number::Float(*f),
                            got: value.clone(),
                        });
                    }
                }
            },
            Validator::LessThan(number) => match number {
                Number::Int(i) => {
                    let value_num = value.as_i64().expect("type validation to have run");
                    if value_num >= *i {
                        fail!(OrmErrorKind::NotLessThan {
                            expected: Number::Int(*i),
                            got: value.clone(),
                        });
                    }
                }
                Number::Float(f) => {
                    let value_num = value.as_f64().expect("type validation to have run");
                    if value_num >= *f {
                        fail!(OrmErrorKind::NotLessThan {
                            expected: Number::Float(*f),
                            got: value.clone(),
                        });
                    }
                }
            },
            Validator::LessThanOrEqual(number) => match number {
                Number::Int(i) => {
                    let value_num = value.as_i64().expect("type validation to have run");
                    if value_num > *i {
                        fail!(OrmErrorKind::NotLessThanOrEqual {
                            expected: Number::Int(*i),
                            got: value.clone(),
                        });
                    }
                }
                Number::Float(f) => {
                    let value_num = value.as_f64().expect("type validation to have run");
                    if value_num > *f {
                        fail!(OrmErrorKind::NotLessThanOrEqual {
                            expected: Number::Float(*f),
                            got: value.clone(),
                        });
                    }
                }
            },
            Validator::Step(i) => {
                let value_num = value.as_i64().expect("type validation to have run");
                if value_num % *i != 0 {
                    fail!(OrmErrorKind::NotStep {
                        expected: Number::Int(*i),
                        got: value.clone(),
                    });
                }
            }
            Validator::Length(size) => {
                let value_str = value.as_str().expect("type validation to have run");
                let size_i64 = i64::try_from(*size).unwrap_or(i64::MAX);
                if value_str.len() != *size {
                    fail!(OrmErrorKind::NotLength {
                        expected: Number::Int(size_i64),
                        got: value.clone(),
                    });
                }
            }
            Validator::MinLength(min) => {
                let value_str = value.as_str().expect("type validation to have run");
                let min_i64 = i64::try_from(*min).unwrap_or(i64::MAX);
                if value_str.len() < *min {
                    fail!(OrmErrorKind::NotMinLength {
                        expected: Number::Int(min_i64),
                        got: value.clone(),
                    });
                }
            }
            Validator::MaxLength(max) => {
                let value_str = value.as_str().expect("type validation to have run");
                let max_i64 = i64::try_from(*max).unwrap_or(i64::MAX);
                if value_str.len() > *max {
                    fail!(OrmErrorKind::NotMaxLength {
                        expected: Number::Int(max_i64),
                        got: value.clone(),
                    });
                }
            }
            Validator::Regex(r) => {
                let value_str = value.as_str().expect("type validation to have run");
                if !regex::Regex::new(r).unwrap().is_match(value_str) {
                    fail!(OrmErrorKind::UnmatchedRegex {
                        got: value.clone(),
                        pattern: r.to_string(),
                    });
                }
            }
        }
    }

    Ok(())
}
