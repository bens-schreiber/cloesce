use std::collections::HashMap;

use ast::{CidlType, CloesceAst, NavigationFieldKind};

use base64::{Engine, prelude::BASE64_STANDARD};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, PartialEq, Serialize)]
pub enum ValidatorErrorKind {
    Undefined,
    Null,
    NonI64,
    NonReal,
    NonString,
    NonBoolean,
    NonDateIso,
    NonBase64,
    NonU8Array,
    InvalidKvObject,
    NonObject,
    InvalidR2Object,
    UnknownDataSource,
    NonArray,
}

/// Runtime type validation, asserting that the structure of a value
/// follows the correlated CidlType.
///
/// - All values must be defined unless `isPartial` is true.
/// - Arrays can be left undefined, which will be interpreted as empty.
/// - Blob types are checked to be b64 encoded
/// - Dates are checked to be valid ISO strings
pub fn validate_cidl_type(
    cidl_type: CidlType,
    value: Option<Value>,
    ast: &CloesceAst,
    partial: bool,
) -> Result<Option<Value>, ValidatorErrorKind> {
    // Json accepts anything
    if cidl_type == CidlType::Json {
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

        return Err(ValidatorErrorKind::Undefined);
    };

    let is_nullable = matches!(&cidl_type, CidlType::Nullable(_));
    if value.is_null() || value == Value::String("null".to_string()) {
        // NOTE: Partial types are always nullable.
        if is_nullable || is_partial {
            return Ok(Some(Value::Null));
        }

        return Err(ValidatorErrorKind::Null);
    }

    let unwrapped_type = match cidl_type {
        CidlType::Nullable(inner) => *inner,
        _ => cidl_type,
    };

    match unwrapped_type {
        CidlType::Integer => match &value {
            Value::Number(num) if num.is_i64() => Ok(Some(value)),
            Value::String(s) if s.parse::<i64>().is_ok() => {
                value = Value::Number(s.parse::<i64>().unwrap().into());
                Ok(Some(value))
            }
            _ => Err(ValidatorErrorKind::NonI64),
        },

        CidlType::Double => match &value {
            Value::Number(num) if num.is_f64() || num.is_i64() => Ok(Some(value)),
            Value::String(s) if s.parse::<f64>().is_ok() => {
                value =
                    Value::Number(serde_json::Number::from_f64(s.parse::<f64>().unwrap()).unwrap());
                Ok(Some(value))
            }
            _ => Err(ValidatorErrorKind::NonReal),
        },

        CidlType::String => value
            .is_string()
            .then_some(Some(value))
            .ok_or(ValidatorErrorKind::NonString),

        CidlType::Boolean => match &value {
            Value::Bool(_) => Ok(Some(value)),
            Value::String(s) if s.eq_ignore_ascii_case("true") => Ok(Some(Value::Bool(true))),
            Value::String(s) if s.eq_ignore_ascii_case("false") => Ok(Some(Value::Bool(false))),
            _ => Err(ValidatorErrorKind::NonBoolean),
        },

        CidlType::DateIso => value
            .as_str()
            .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
            .map(|_| Some(value))
            .ok_or(ValidatorErrorKind::NonDateIso),

        CidlType::Blob => match &value {
            Value::String(s) => BASE64_STANDARD
                .decode(s)
                .ok()
                .map(|bytes| {
                    Some(Value::Array(
                        bytes.into_iter().map(|b| Value::Number(b.into())).collect(),
                    ))
                })
                .ok_or(ValidatorErrorKind::NonBase64),

            Value::Array(arr) => {
                // everything must be u8 (0-255)
                if arr.iter().all(|v| v.is_u64() && v.as_u64().unwrap() <= 255) {
                    Ok(Some(value))
                } else {
                    Err(ValidatorErrorKind::NonU8Array)
                }
            }

            _ => Err(ValidatorErrorKind::NonBase64),
        },

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

            value
                .as_object()
                .and_then(|obj| serde_json::from_value::<R2Object>(Value::Object(obj.clone())).ok())
                .map(|_| Some(value))
                .ok_or(ValidatorErrorKind::InvalidR2Object)
        }

        CidlType::DataSource { model_name } => {
            let model = ast.models.get(&model_name).unwrap();
            let Some(value_str) = value.as_str() else {
                return Err(ValidatorErrorKind::NonString);
            };

            // TODO: adjust this when data sources are revamped
            // for now we allow "none"
            if value_str == "none" || model.data_sources.iter().any(|ds| ds.name == value_str) {
                return Ok(Some(value));
            }

            Err(ValidatorErrorKind::UnknownDataSource)
        }

        CidlType::KvObject(inner) => {
            let obj = value.as_object_mut().ok_or(ValidatorErrorKind::NonObject)?;
            let key = obj.remove("key");
            let raw = obj.remove("raw");
            let metadata = obj.remove("metadata");

            let mut new_obj = serde_json::Map::<String, Value>::new();

            // Key must exist and be a string
            if !partial && !matches!(key, Some(Value::String(_))) {
                return Err(ValidatorErrorKind::InvalidKvObject);
            }
            new_obj.insert("key".to_string(), key.unwrap_or(Value::Null));

            // Metadata must be an object or null if it exists
            if let Some(metadata) = metadata.to_owned()
                && !(metadata.is_object() || metadata.is_null())
                && !partial
            {
                return Err(ValidatorErrorKind::InvalidKvObject);
            }
            new_obj.insert("metadata".to_string(), metadata.unwrap_or(Value::Null));

            // Validate raw value
            let raw = validate_cidl_type(*inner, raw, ast, partial)?;
            if let Some(raw) = raw {
                new_obj.insert("raw".to_string(), raw);
            }

            Ok(Some(Value::Object(new_obj)))
        }

        CidlType::Object { name } | CidlType::Partial { object_name: name } => {
            let obj = value.as_object_mut().ok_or(ValidatorErrorKind::NonObject)?;
            let mut new_obj = serde_json::Map::<String, Value>::new();

            // Handle Plain Old Objects
            if let Some(poo) = ast.poos.get(&name) {
                for attr in &poo.fields {
                    let attr_value = obj.remove(&attr.name);
                    let res =
                        validate_cidl_type(attr.cidl_type.clone(), attr_value, ast, is_partial)?;

                    if let Some(res) = res {
                        new_obj.insert(attr.name.clone(), res);
                    }
                }

                return Ok(Some(Value::Object(new_obj)));
            }

            // Handle Models
            let model = ast.models.get(&name).unwrap();
            let obj = value.as_object_mut().ok_or(ValidatorErrorKind::NonObject)?;

            for key_param in &model.key_fields {
                let key_param_value = obj.remove(key_param);
                let res = validate_cidl_type(CidlType::String, key_param_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(key_param.clone(), res);
                }
            }

            for (col, _) in model.all_columns() {
                let col_value = obj.remove(&col.field.name);
                let res =
                    validate_cidl_type(col.field.cidl_type.clone(), col_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(col.field.name.clone(), res);
                }
            }

            for nav in &model.navigation_fields {
                let nav_value = obj.remove(&nav.field.name);

                let nav_cidl_type = match nav.kind {
                    NavigationFieldKind::ManyToMany | NavigationFieldKind::OneToMany { .. } => {
                        CidlType::Array(Box::new(CidlType::Object {
                            name: nav.model_reference.clone(),
                        }))
                    }

                    _ => CidlType::Object {
                        name: nav.model_reference.clone(),
                    },
                };

                let res = validate_cidl_type(nav_cidl_type, nav_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(nav.field.name.clone(), res);
                }
            }

            for kv_obj_meta in &model.kv_fields {
                let kv_obj_value = obj.remove(&kv_obj_meta.field.name);

                let cidl_type = if kv_obj_meta.list_prefix {
                    CidlType::Paginated(Box::new(CidlType::KvObject(Box::new(
                        kv_obj_meta.field.cidl_type.clone(),
                    ))))
                } else {
                    CidlType::KvObject(Box::new(kv_obj_meta.field.cidl_type.clone()))
                };

                let res = validate_cidl_type(cidl_type, kv_obj_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(kv_obj_meta.field.name.clone(), res);
                }
            }

            for r2_obj_meta in &model.r2_fields {
                let r2_obj_value = obj.remove(&r2_obj_meta.field.name);

                let cidl_type = if r2_obj_meta.list_prefix {
                    CidlType::Paginated(Box::new(CidlType::R2Object))
                } else {
                    CidlType::R2Object
                };

                let res = validate_cidl_type(cidl_type, r2_obj_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(r2_obj_meta.field.name.clone(), res);
                }
            }

            Ok(Some(Value::Object(new_obj)))
        }

        CidlType::Array(cidl_type) => {
            let arr = value.as_array().ok_or(ValidatorErrorKind::NonArray)?;
            let mut new_arr = Vec::<Value>::new();
            for item in arr {
                let res =
                    validate_cidl_type(*cidl_type.clone(), Some(item.clone()), ast, is_partial)?;

                if let Some(res) = res {
                    new_arr.push(res);
                }
            }
            Ok(Some(Value::Array(new_arr)))
        }

        CidlType::Paginated(inner) => {
            let obj = value.as_object_mut().ok_or(ValidatorErrorKind::NonObject)?;
            let mut new_obj = serde_json::Map::<String, Value>::new();

            // Validate results array
            let results = obj.remove("results");
            let results_value =
                validate_cidl_type(CidlType::Array(inner), results, ast, is_partial)?;
            if let Some(results_value) = results_value {
                new_obj.insert("results".to_string(), results_value);
            }

            // Validate cursor (string | null)
            let cursor = obj.remove("cursor");
            if let Some(cursor_value) = cursor {
                if !cursor_value.is_string() && !cursor_value.is_null() {
                    return Err(ValidatorErrorKind::NonString);
                }
                new_obj.insert("cursor".to_string(), cursor_value);
            } else {
                new_obj.insert("cursor".to_string(), Value::Null);
            }

            // Validate complete (boolean)
            let complete = obj.remove("complete");
            let complete_value = validate_cidl_type(CidlType::Boolean, complete, ast, is_partial)?;
            if let Some(complete_value) = complete_value {
                new_obj.insert("complete".to_string(), complete_value);
            }

            Ok(Some(Value::Object(new_obj)))
        }

        _ => unimplemented!(),
    }
}
