use std::collections::HashMap;

use idl::{CidlType, CloesceIdl, Number, ValidatedField, Validator};

use base64::{Engine, prelude::BASE64_STANDARD};
use frontend::fmt_cidl_type;
use serde::Deserialize;
use serde_json::Value;

use crate::{OrmErrorKind, fail};

/// Runtime type validation, asserting that the structure of a JSON value
/// matches the structure of the provided CIDL type.
///
/// Additionally, runs any validators on the value (should it be an [idl::ValidatedField])
pub fn validate_cidl_type(
    field: &ValidatedField,
    value: Option<Value>,
    idl: &CloesceIdl,
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
            let valid = value.as_str().is_some_and(is_valid_rfc3339);
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
                uploaded: String,
                custom_metadata: Option<HashMap<String, String>>,
            }

            let valid = value
                .as_object()
                .and_then(|obj| R2Object::deserialize(obj).ok())
                .is_some();
            if valid {
                Some(value)
            } else {
                fail!(type_mismatch_err(value))
            }
        }

        CidlType::KvObject(inner) => {
            if !value.is_object() {
                fail!(type_mismatch_err(value));
            }
            let obj = value.as_object_mut().unwrap();
            let raw = obj.remove("raw");
            let metadata = obj.remove("metadata");

            let mut new_obj = serde_json::Map::<String, Value>::new();

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
                idl,
                partial,
            )?;
            if let Some(raw) = raw {
                new_obj.insert("raw".to_string(), raw);
            }

            return Ok(Some(Value::Object(new_obj)));
        }

        // Plain old objects
        CidlType::Object { name } | CidlType::Partial { object_name: name }
            if let Some(poo) = idl.poos.get(name) =>
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
                    idl,
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
            let model = idl.models.get(name).unwrap();

            for (col, _) in model.all_columns() {
                let col_value = obj.remove(col.field.name.as_ref());
                let res = validate_cidl_type(&col.field, col_value, idl, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(col.field.name.to_string(), res);
                }
            }

            for route_field in &model.route_fields {
                let route_value = obj.remove(route_field.name.as_ref());
                let res = validate_cidl_type(route_field, route_value, idl, is_partial)?;

                if let Some(res) = res {
                    new_obj.insert(route_field.name.to_string(), res);
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
                    idl,
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

                let res = validate_cidl_type(&kv_field.field, kv_field_value, idl, is_partial)?;

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
                    idl,
                    is_partial,
                )?;

                if let Some(res) = res {
                    new_obj.insert(r2_obj_meta.field.name.to_string(), res);
                }
            }

            Some(Value::Object(new_obj))
        }

        CidlType::Array(cidl_type) => {
            let Value::Array(arr) = value else {
                fail!(type_mismatch_err(value));
            };
            let mut new_arr = Vec::<Value>::with_capacity(arr.len());
            let field = ValidatedField {
                name: field.name.clone(),
                cidl_type: *cidl_type.clone(),
                validators: field.validators.clone(),
            };
            for item in arr {
                let res = validate_cidl_type(&field, Some(item), idl, is_partial)?;
                if let Some(res) = res {
                    new_arr.push(res);
                }
            }
            Some(Value::Array(new_arr))
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
                // TODO: this recompiles the regex on every value (once per array
                // element).
                if !regex_lite::Regex::new(r).unwrap().is_match(value_str) {
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

// Adapted from chrono 0.4.44 (MIT/Apache-2.0, Copyright 2014--2026 Kang Seonghoon and contributors)
// https://github.com/chronotope/chrono
// See: src/format/parse.rs `parse_rfc3339`, `digit`
// See: src/format/scan.rs `nanosecond`, `timezone_offset`
fn is_valid_rfc3339(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 19 {
        return false;
    }

    #[inline]
    fn digit(b: &[u8], i: usize) -> Option<u8> {
        match b[i] {
            c @ b'0'..=b'9' => Some(c - b'0'),
            _ => None,
        }
    }

    // date-fullyear "-" date-month "-" date-mday
    let _year = (|| {
        Some(
            digit(b, 0)? as u16 * 1000
                + digit(b, 1)? as u16 * 100
                + digit(b, 2)? as u16 * 10
                + digit(b, 3)? as u16,
        )
    })();
    if b[4] != b'-' {
        return false;
    }
    let month = (|| Some(digit(b, 5)? * 10 + digit(b, 6)?))();
    if b[7] != b'-' {
        return false;
    }
    let day = (|| Some(digit(b, 8)? * 10 + digit(b, 9)?))();

    let (Some(_year), Some(month), Some(day)) = (_year, month, day) else {
        return false;
    };
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return false;
    }

    // "T" / "t" / " "
    if !matches!(b[10], b'T' | b't' | b' ') {
        return false;
    }

    // time-hour ":" time-minute ":" time-second
    let hour = (|| Some(digit(b, 11)? * 10 + digit(b, 12)?))();
    if b[13] != b':' {
        return false;
    }
    let min = (|| Some(digit(b, 14)? * 10 + digit(b, 15)?))();
    if b[16] != b':' {
        return false;
    }
    let sec = (|| Some(digit(b, 17)? * 10 + digit(b, 18)?))();

    let (Some(hour), Some(min), Some(sec)) = (hour, min, sec) else {
        return false;
    };
    // sec == 60 is allowed for leap seconds per RFC 3339
    if hour > 23 || min > 59 || sec > 60 {
        return false;
    }

    // [time-secfrac]: "." 1*DIGIT
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        if i >= b.len() || !b[i].is_ascii_digit() {
            return false;
        }
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
    }

    // time-offset: "Z" / time-numoffset
    match b.get(i) {
        Some(b'Z' | b'z') => i += 1,
        Some(b'+' | b'-') => {
            // time-numoffset: ("+" / "-") time-hour ":" time-minute
            if i + 6 > b.len() {
                return false;
            }
            let oh = (|| Some(digit(b, i + 1)? * 10 + digit(b, i + 2)?))();
            let om = (|| Some(digit(b, i + 4)? * 10 + digit(b, i + 5)?))();
            match (oh, om) {
                (Some(oh), Some(om)) if oh <= 23 && om <= 59 && b[i + 3] == b':' => {}
                _ => return false,
            }
            i += 6;
        }
        _ => return false,
    }

    i == b.len()
}

#[cfg(test)]
mod tests {
    use super::is_valid_rfc3339;

    #[test]
    fn valid_rfc3339() {
        assert!(is_valid_rfc3339("2024-01-15T10:30:00Z"));
        assert!(is_valid_rfc3339("2024-01-15t10:30:00z"));
        assert!(is_valid_rfc3339("2024-01-15 10:30:00Z"));
        assert!(is_valid_rfc3339("2024-01-15T10:30:00+05:30"));
        assert!(is_valid_rfc3339("2024-01-15T10:30:00-08:00"));
        assert!(is_valid_rfc3339("2024-01-15T10:30:00.123Z"));
        assert!(is_valid_rfc3339("2024-01-15T10:30:00.123456789Z"));
        assert!(is_valid_rfc3339("2024-01-15T23:59:60Z")); // leap second
    }

    #[test]
    fn invalid_rfc3339() {
        assert!(!is_valid_rfc3339(""));
        assert!(!is_valid_rfc3339("not-a-date"));
        assert!(!is_valid_rfc3339("2024-01-15 10:30:00")); // missing timezone
        assert!(!is_valid_rfc3339("2024-01-15T10:30:00")); // missing timezone
        assert!(!is_valid_rfc3339("2024-13-15T10:30:00Z")); // month 13
        assert!(!is_valid_rfc3339("2024-01-32T10:30:00Z")); // day 32
        assert!(!is_valid_rfc3339("2024-01-15T25:30:00Z")); // hour 25
        assert!(!is_valid_rfc3339("2024-01-15T10:60:00Z")); // minute 60
        assert!(!is_valid_rfc3339("2024-01-15T10:30:00.Z")); // dot with no digits
        assert!(!is_valid_rfc3339("2024-01-15T10:30:00Zextra")); // trailing chars
        assert!(!is_valid_rfc3339("2024-01-15T10:30:00+25:00")); // offset hour 25
        assert!(!is_valid_rfc3339("2024-01-15T10:30:00+05:60")); // offset min 60
    }
}
