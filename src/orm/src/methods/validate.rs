use std::collections::HashMap;

use ast::{CidlType, CloesceAst, NavigationPropertyKind};

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
    // JsonValue accepts anything
    if cidl_type == CidlType::JsonValue {
        return Ok(value);
    }

    let is_partial = partial || matches!(&cidl_type, CidlType::Partial(_));

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

        CidlType::Real => match &value {
            Value::Number(num) if num.is_f64() || num.is_i64() => Ok(Some(value)),
            Value::String(s) if s.parse::<f64>().is_ok() => {
                value =
                    Value::Number(serde_json::Number::from_f64(s.parse::<f64>().unwrap()).unwrap());
                Ok(Some(value))
            }
            _ => Err(ValidatorErrorKind::NonReal),
        },

        CidlType::Text => value
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
                // everythign must be u8 (0-255)
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

        CidlType::DataSource(model_name) => {
            let model = ast.models.get(&model_name).unwrap();
            let Some(value_str) = value.as_str() else {
                return Err(ValidatorErrorKind::NonString);
            };

            // TODO: adjust this when data sources are revamped
            // for now we allow "none"
            if value_str == "none" || model.data_sources.contains_key(value_str) {
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
            new_obj.insert("key".to_string(), key.unwrap_or(Value::Null).clone());

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

            return Ok(Some(Value::Object(new_obj)));
        }

        CidlType::Object(name) | CidlType::Partial(name) => {
            let obj = value.as_object_mut().ok_or(ValidatorErrorKind::NonObject)?;
            let mut new_obj = serde_json::Map::<String, Value>::new();

            // Handle Plain Old Objects
            if let Some(poo) = ast.poos.get(&name) {
                for attr in &poo.attributes {
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

            if let Some(pk) = &model.primary_key {
                let pk_value = obj.remove(&pk.name);
                let res = validate_cidl_type(pk.cidl_type.clone(), pk_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(pk.name.clone(), res);
                }
            }

            for key_param in &model.key_params {
                let key_param_value = obj.remove(key_param);
                let res = validate_cidl_type(CidlType::Text, key_param_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(key_param.clone(), res);
                }
            }

            for col in &model.columns {
                let col_value = obj.remove(&col.value.name);
                let res =
                    validate_cidl_type(col.value.cidl_type.clone(), col_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(col.value.name.clone(), res);
                }
            }

            for nav in &model.navigation_properties {
                let nav_value = obj.remove(&nav.var_name);

                let nav_cidl_type = match nav.kind {
                    NavigationPropertyKind::ManyToMany
                    | NavigationPropertyKind::OneToMany { .. } => {
                        CidlType::Array(Box::new(CidlType::Object(nav.model_reference.clone())))
                    }

                    _ => CidlType::Object(nav.model_reference.clone()),
                };

                let res = validate_cidl_type(nav_cidl_type, nav_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(nav.var_name.clone(), res);
                }
            }

            for kv_obj_meta in &model.kv_objects {
                let kv_obj_value = obj.remove(&kv_obj_meta.value.name);
                let res = validate_cidl_type(
                    CidlType::KvObject(Box::new(kv_obj_meta.value.cidl_type.clone())),
                    kv_obj_value,
                    ast,
                    is_partial,
                )?;

                if let Some(res) = res {
                    new_obj.insert(kv_obj_meta.value.name.clone(), res);
                }
            }

            for r2_obj_meta in &model.r2_objects {
                let r2_obj_value = obj.remove(&r2_obj_meta.var_name);
                let res = validate_cidl_type(CidlType::R2Object, r2_obj_value, ast, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(r2_obj_meta.var_name.clone(), res);
                }
            }

            return Ok(Some(Value::Object(new_obj)));
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
            return Ok(Some(Value::Array(new_arr)));
        }

        _ => unimplemented!(),
    }
}

#[cfg(test)]
mod tests {
    use ast::{CidlType, NavigationPropertyKind};
    use base64::Engine;
    use generator_test::{ModelBuilder, create_ast};
    use serde_json::{Value, json};

    use crate::methods::validate::{ValidatorErrorKind, validate_cidl_type};

    #[test]
    fn undefined() {
        // Missing required field
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(CidlType::Text, None, &ast, false);
            assert!(matches!(result, Err(ValidatorErrorKind::Undefined)));
        }

        // Allowed for partial types
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(
                CidlType::Partial("SomeModel".to_string()),
                None,
                &ast,
                false,
            );
            assert!(result.is_ok());
            assert!(result.unwrap().is_none());
        }

        // Arrays return empty array when undefined, even if not partial
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(
                CidlType::Array(Box::new(CidlType::Integer)),
                None,
                &ast,
                false,
            );
            assert_eq!(result.unwrap(), Some(Value::Array(vec![])));
        }
    }

    #[test]
    fn null_value() {
        // Null required field
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(CidlType::Text, Some(Value::Null), &ast, false);
            assert!(matches!(result, Err(ValidatorErrorKind::Null)));
        }

        // Null string for non-nullable type
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(CidlType::Text, Some(json!("null")), &ast, false);
            assert!(matches!(result, Err(ValidatorErrorKind::Null)));
        }

        // Null allowed for nullable type
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(
                CidlType::nullable(CidlType::Text),
                Some(Value::Null),
                &ast,
                false,
            );
            assert_eq!(result.unwrap(), Some(Value::Null));
        }

        // Null allowed for partial type
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(
                CidlType::Partial("SomeModel".to_string()),
                Some(Value::Null),
                &ast,
                false,
            );
            assert_eq!(result.unwrap(), Some(Value::Null));
        }
    }

    #[test]
    fn integer() {
        // Non integer returns error
        {
            let ast = create_ast(vec![]);
            let result =
                validate_cidl_type(CidlType::Integer, Some(json!("not_an_int")), &ast, false);
            assert!(matches!(result, Err(ValidatorErrorKind::NonI64)));
        }

        // Floats return error
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(CidlType::Integer, Some(json!(3.14)), &ast, false);
            assert!(matches!(result, Err(ValidatorErrorKind::NonI64)));
        }

        // Integers pass
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(CidlType::Integer, Some(json!(42)), &ast, false);
            assert_eq!(result.unwrap(), Some(json!(42)));
        }
    }

    #[test]
    fn real() {
        // Float passes
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(CidlType::Real, Some(json!(3.14)), &ast, false);
            assert_eq!(result.unwrap(), Some(json!(3.14)));
        }

        // Integer passes
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(CidlType::Real, Some(json!(42)), &ast, false);
            assert_eq!(result.unwrap(), Some(json!(42)));
        }
    }

    #[test]
    fn date_iso() {
        // Non ISO string returns error
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(
                CidlType::DateIso,
                Some(json!("2024-01-15 10:30:00")), // space instead of T and no timezone
                &ast,
                false,
            );
            assert!(matches!(result, Err(ValidatorErrorKind::NonDateIso)));
        }

        // Valid ISO string with timezone passes
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(
                CidlType::DateIso,
                Some(json!("2024-01-15T10:30:00Z")),
                &ast,
                false,
            );
            assert_eq!(result.unwrap(), Some(json!("2024-01-15T10:30:00Z")));
        }
    }

    #[test]
    fn blob() {
        // Invalid b64 returns error
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(
                CidlType::Blob,
                Some(json!("not valid base64!!!")),
                &ast,
                false,
            );
            assert!(matches!(result, Err(ValidatorErrorKind::NonBase64)));
        }

        // Valid B64 passes
        {
            let ast = create_ast(vec![]);
            let encoded = base64::prelude::BASE64_STANDARD.encode(b"hello world");
            let result = validate_cidl_type(CidlType::Blob, Some(json!(encoded)), &ast, false);
            assert!(result.is_ok());
            assert_eq!(
                result.unwrap(),
                Some(json!(
                    base64::prelude::BASE64_STANDARD.decode(encoded).unwrap()
                ))
            );
        }
    }

    #[test]
    fn kv() {
        // Missing key
        {
            let ast = create_ast(vec![]);
            let value = json!({
                "raw": "hello"
            });
            let result = validate_cidl_type(
                CidlType::KvObject(Box::new(CidlType::Text)),
                Some(value),
                &ast,
                false,
            );
            assert!(matches!(result, Err(ValidatorErrorKind::InvalidKvObject)));
        }

        // Non object metadata
        {
            let ast = create_ast(vec![]);
            let value = json!({
                "key": "my-key",
                "raw": "hello",
                "metadata": "not-an-object"
            });
            let result = validate_cidl_type(
                CidlType::KvObject(Box::new(CidlType::Text)),
                Some(value),
                &ast,
                false,
            );
            assert!(matches!(result, Err(ValidatorErrorKind::InvalidKvObject)));
        }

        // Valid KV passes
        {
            let ast = create_ast(vec![]);
            let value = json!({
                "key": "my-key",
                "raw": "hello"
            });
            let result = validate_cidl_type(
                CidlType::KvObject(Box::new(CidlType::Text)),
                Some(value),
                &ast,
                false,
            );
            assert!(result.is_ok());
        }
    }

    #[test]
    fn array() {
        // Non array returns error
        {
            let ast = create_ast(vec![]);
            let result = validate_cidl_type(
                CidlType::array(CidlType::Integer),
                Some(json!("not an array")),
                &ast,
                false,
            );
            assert!(matches!(result, Err(ValidatorErrorKind::NonArray)));
        }

        // Valid array passes
        {
            // Arrange
            let ast = create_ast(vec![]);

            // Act
            let result = validate_cidl_type(
                CidlType::array(CidlType::Integer),
                Some(json!([1, 2, 3])),
                &ast,
                false,
            );

            // Assert
            assert_eq!(result.unwrap(), Some(json!([1, 2, 3])));
        }
    }

    #[test]
    fn r2() {
        // Missing required fields returns error
        {
            // Arrange
            let ast = create_ast(vec![]);
            let value = json!({
                "key": "some-key"
                // missing version, size, etag, http_etag, uploaded
            });

            // Act
            let result = validate_cidl_type(CidlType::R2Object, Some(value), &ast, false);

            // Assert
            assert!(matches!(result, Err(ValidatorErrorKind::InvalidR2Object)));
        }

        // Non object returns error
        {
            // Arrange
            let ast = create_ast(vec![]);

            // Act
            let result = validate_cidl_type(
                CidlType::R2Object,
                Some(json!("just a string")),
                &ast,
                false,
            );

            // Assert
            assert!(matches!(result, Err(ValidatorErrorKind::InvalidR2Object)));
        }

        // Invalid elements propogate error
        {
            // Arrange
            let ast = create_ast(vec![]);

            // Act
            let result = validate_cidl_type(
                CidlType::array(CidlType::Integer),
                Some(json!(["not", "integers"])),
                &ast,
                false,
            );

            // Assert
            assert!(matches!(result, Err(ValidatorErrorKind::NonI64)));
        }

        // Valid R2 passes
        {
            // Arrange
            let ast = create_ast(vec![]);
            let value = json!({
                "key": "uploads/photo.jpg",
                "version": "v1",
                "size": 1024,
                "etag": "abc123",
                "http_etag": "\"abc123\"",
                "uploaded": "2024-01-15T10:30:00Z",
                "custom_metadata": null
            });

            // Act
            let result = validate_cidl_type(CidlType::R2Object, Some(value), &ast, false);

            // Assert
            assert!(result.is_ok());
        }
    }

    #[test]
    fn data_source() {
        // Unknown data source
        {
            // Arrange
            let horse = ModelBuilder::new("Horse").id_pk().build();

            let ast = generator_test::create_ast(vec![horse]);

            // Act
            let result = validate_cidl_type(
                CidlType::DataSource("Horse".to_string()),
                Some(json!("nonexistent_source")),
                &ast,
                false,
            );

            // Assert
            assert!(matches!(result, Err(ValidatorErrorKind::UnknownDataSource)));
        }

        // "none" is valid data source
        {
            // Arrange
            let horse = ModelBuilder::new("Horse").id_pk().build();

            let ast = generator_test::create_ast(vec![horse]);

            // Act
            let result = validate_cidl_type(
                CidlType::DataSource("Horse".to_string()),
                Some(json!("none")),
                &ast,
                false,
            );

            // Assert
            assert_eq!(result.unwrap(), Some(json!("none")));
        }
    }

    #[test]
    fn objects_partials() {
        // Invalid column propogates
        {
            // Arrange
            let horse = ModelBuilder::new("Horse")
                .id_pk()
                .col("name", CidlType::Text, None)
                .build();

            let ast = generator_test::create_ast(vec![horse]);

            let value = json!({
                "id": 1,
                "name": 99  // should be Text
            });

            // Act
            let result = validate_cidl_type(
                CidlType::Object("Horse".to_string()),
                Some(value),
                &ast,
                false,
            );

            // Assert
            assert!(matches!(result, Err(ValidatorErrorKind::NonString)));
        }

        // Missing key param
        {
            // Arrange
            let horse = ModelBuilder::new("Horse")
                .id_pk()
                .key_param("nickname")
                .build();

            let ast = generator_test::create_ast(vec![horse]);

            let value = json!({
                "id": 1
                // missing nickname key param
            });

            // Act
            let result = validate_cidl_type(
                CidlType::Object("Horse".to_string()),
                Some(value),
                &ast,
                false,
            );

            // Assert
            assert!(matches!(result, Err(ValidatorErrorKind::Undefined)));
        }

        // Valid object passes
        {
            // Arrange
            let horse = ModelBuilder::new("Horse")
                .id_pk()
                .col("name", CidlType::Text, None)
                .key_param("color")
                .nav_p(
                    "riders",
                    "Rider",
                    NavigationPropertyKind::OneToMany {
                        column_reference: "horse_id".into(),
                    },
                )
                .build();

            let rider = ModelBuilder::new("Rider")
                .id_pk()
                .col("nickname", CidlType::Text, None)
                .build();

            let ast = generator_test::create_ast(vec![horse, rider]);

            let value = json!({
                "id": 1,
                "name": "Shadowfax",
                "color": "gray",
                "riders": []
            });

            // Act
            let result = validate_cidl_type(
                CidlType::Object("Horse".to_string()),
                Some(value.clone()),
                &ast,
                false,
            );

            // Assert
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), Some(value));
        }

        // Partial type allows missing fields
        {
            // Arrange
            let horse = ModelBuilder::new("Horse")
                .id_pk()
                .col("name", CidlType::Text, None)
                .build();

            let ast = generator_test::create_ast(vec![horse]);

            // A partial object with only the pk — `name` is absent
            let value = json!({ "id": 1 });

            // Act
            let result = validate_cidl_type(
                CidlType::Partial("Horse".to_string()),
                Some(value),
                &ast,
                false,
            );

            // Assert
            assert!(result.is_ok());
            let obj = result.unwrap();
            assert_eq!(obj, Some(json!({ "id": 1 })));
        }
    }

    #[test]
    fn json_value_type_accepts_anything() {
        // Arrange
        let ast = create_ast(vec![]);

        for val in [
            json!(null),
            json!(1),
            json!("text"),
            json!([1, 2]),
            json!({"a": 1}),
        ] {
            // Act
            let result = validate_cidl_type(CidlType::JsonValue, Some(val.clone()), &ast, false);

            // Assert
            assert_eq!(result.unwrap(), Some(val));
        }
    }
}
