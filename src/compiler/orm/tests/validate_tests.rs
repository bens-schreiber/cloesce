use base64::Engine;
use compiler_test::src_to_idl;
use idl::{CidlType, CloesceIdl, Number, ValidatedField, Validator};
use orm::{OrmErrorKind, validate::validate_cidl_type};
use serde_json::{Value, json};

fn empty_idl() -> CloesceIdl<'static> {
    src_to_idl(
        r#"
        env {
            d1 { db }
        }
        "#,
    )
}

fn make_field<'a>(cidl_type: CidlType<'a>, validators: Vec<Validator<'a>>) -> ValidatedField<'a> {
    ValidatedField {
        name: "test".into(),
        cidl_type,
        validators,
    }
}

fn validate(
    cidl_type: CidlType,
    value: Option<Value>,
    idl: &CloesceIdl,
) -> Result<Option<Value>, OrmErrorKind> {
    validate_cidl_type(&make_field(cidl_type, vec![]), value, idl, false)
}

fn validate_with(
    cidl_type: CidlType,
    validators: &[Validator],
    value: Option<Value>,
    idl: &CloesceIdl,
) -> Result<Option<Value>, OrmErrorKind> {
    validate_cidl_type(
        &make_field(cidl_type, validators.to_vec()),
        value,
        idl,
        false,
    )
}

#[test]
fn undefined() {
    // Missing required field
    {
        let idl = empty_idl();
        let result = validate(CidlType::String, None, &idl);
        assert!(matches!(result, Err(OrmErrorKind::MissingField { .. })));
    }

    // Allowed for partial types
    {
        let idl = src_to_idl(
            r#"
            env {
                d1 { db }
            }

            [use db]
            model SomeModel {
                primary {
                    id: int
                }
            }
        "#,
        );
        let result = validate(
            CidlType::Partial {
                object_name: "SomeModel",
            },
            None,
            &idl,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // Arrays return empty array when undefined, even if not partial
    {
        let idl = empty_idl();
        let result = validate(CidlType::Array(Box::new(CidlType::Int)), None, &idl);
        assert_eq!(result.unwrap(), Some(Value::Array(vec![])));
    }
}

#[test]
fn null_value() {
    // Null required field
    {
        let idl = empty_idl();
        let result = validate(CidlType::String, Some(Value::Null), &idl);
        assert!(matches!(result, Err(OrmErrorKind::MissingField { .. })));
    }

    // Null string for non-nullable type
    {
        let idl = empty_idl();
        let result = validate(CidlType::String, Some(json!("null")), &idl);
        assert!(matches!(result, Err(OrmErrorKind::MissingField { .. })));
    }

    // Null allowed for nullable type
    {
        let idl = empty_idl();
        let result = validate(
            CidlType::nullable(CidlType::String),
            Some(Value::Null),
            &idl,
        );
        assert_eq!(result.unwrap(), Some(Value::Null));
    }

    // Null allowed for partial type
    {
        let idl = src_to_idl(
            r#"
            env {
                d1 { db }
            }

            [use db]
            model SomeModel {
                primary {
                    id: int
                }
            }
        "#,
        );
        let result = validate(
            CidlType::Partial {
                object_name: "SomeModel",
            },
            Some(Value::Null),
            &idl,
        );
        assert_eq!(result.unwrap(), Some(Value::Null));
    }
}

#[test]
fn integer() {
    // Non integer returns error
    {
        let idl = empty_idl();
        let result = validate(CidlType::Int, Some(json!("not_an_int")), &idl);
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }

    // Floats return error
    {
        let idl = empty_idl();
        let result = validate(CidlType::Int, Some(json!(3.01)), &idl);
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }

    // Integers pass
    {
        let idl = empty_idl();
        let result = validate(CidlType::Int, Some(json!(42)), &idl);
        assert_eq!(result.unwrap(), Some(json!(42)));
    }
}

#[test]
fn real() {
    // Float passes
    {
        let idl = empty_idl();
        let result = validate(CidlType::Real, Some(json!(3.01)), &idl);
        assert_eq!(result.unwrap(), Some(json!(3.01)));
    }

    // Int passes
    {
        let idl = empty_idl();
        let result = validate(CidlType::Real, Some(json!(42)), &idl);
        assert_eq!(result.unwrap(), Some(json!(42)));
    }
}

#[test]
fn date_iso() {
    // Non ISO string returns error
    {
        let idl = empty_idl();
        let result = validate(CidlType::DateIso, Some(json!("2024-01-15 10:30:00")), &idl);
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }

    // Valid ISO string with timezone passes
    {
        let idl = empty_idl();
        let result = validate(CidlType::DateIso, Some(json!("2024-01-15T10:30:00Z")), &idl);
        assert_eq!(result.unwrap(), Some(json!("2024-01-15T10:30:00Z")));
    }
}

#[test]
fn blob() {
    // Invalid b64 returns error
    {
        let idl = empty_idl();
        let result = validate(CidlType::Blob, Some(json!("not valid base64!!!")), &idl);
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }

    // Valid B64 passes
    {
        let idl = empty_idl();
        let encoded = base64::prelude::BASE64_STANDARD.encode(b"hello world");
        let result = validate(CidlType::Blob, Some(json!(encoded)), &idl);
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
        let idl = empty_idl();
        let value = json!({
            "raw": "hello"
        });
        let result = validate(
            CidlType::KvObject(Box::new(CidlType::String)),
            Some(value),
            &idl,
        );
        assert!(matches!(result, Err(OrmErrorKind::MissingField { .. })));
    }

    // Non object metadata
    {
        let idl = empty_idl();
        let value = json!({
            "key": "my-key",
            "raw": "hello",
            "metadata": "not-an-object"
        });
        let result = validate(
            CidlType::KvObject(Box::new(CidlType::String)),
            Some(value),
            &idl,
        );
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }

    // Valid KV passes
    {
        let idl = empty_idl();
        let value = json!({
            "key": "my-key",
            "raw": "hello"
        });
        let result = validate(
            CidlType::KvObject(Box::new(CidlType::String)),
            Some(value),
            &idl,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn array() {
    // Non array returns error
    {
        let idl = empty_idl();
        let result = validate(
            CidlType::array(CidlType::Int),
            Some(json!("not an array")),
            &idl,
        );
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }

    // Valid array passes
    {
        let idl = empty_idl();
        let result = validate(CidlType::array(CidlType::Int), Some(json!([1, 2, 3])), &idl);
        assert_eq!(result.unwrap(), Some(json!([1, 2, 3])));
    }
}

#[test]
fn r2() {
    // Missing required fields returns error
    {
        let idl = empty_idl();
        let value = json!({
            "key": "some-key"
        });
        let result = validate(CidlType::R2Object, Some(value), &idl);
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }

    // Non object returns error
    {
        let idl = empty_idl();
        let result = validate(CidlType::R2Object, Some(json!("just a string")), &idl);
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }

    // Invalid elements propagate error
    {
        let idl = empty_idl();
        let result = validate(
            CidlType::array(CidlType::Int),
            Some(json!(["not", "integers"])),
            &idl,
        );
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }

    // Valid R2 passes
    {
        let idl = empty_idl();
        let value = json!({
            "key": "uploads/photo.jpg",
            "version": "v1",
            "size": 1024,
            "etag": "abc123",
            "http_etag": "\"abc123\"",
            "uploaded": "2024-01-15T10:30:00Z",
            "custom_metadata": null
        });
        let result = validate(CidlType::R2Object, Some(value), &idl);
        assert!(result.is_ok());
    }
}

#[test]
fn objects_partials() {
    // Invalid column propagates
    {
        let idl = src_to_idl(
            r#"
            env {
                d1 { db }
            }

            [use db]
            model Horse {
                primary {
                    id: int
                }

                name: string
            }
        "#,
        );
        let value = json!({
            "id": 1,
            "name": 99
        });
        let result = validate(CidlType::Object { name: "Horse" }, Some(value), &idl);
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }

    // Valid object passes
    {
        let idl = src_to_idl(
            r#"
            env {
                d1 { db }
            }

            [use db]
            model Horse {
                primary {
                    id: int
                }

                name: string

                nav(Rider::horseId) {
                    riders
                }
            }

            [use db]
            model Rider {
                primary {
                    id: int
                }

                foreign(Horse::id) {
                    horseId
                }

                nickname: string
            }
        "#,
        );
        let value = json!({
            "id": 1,
            "name": "Shadowfax",
            "riders": []
        });
        let result = validate(
            CidlType::Object { name: "Horse" },
            Some(value.clone()),
            &idl,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(value));
    }

    // Partial type allows missing fields
    {
        let idl = src_to_idl(
            r#"
            env {
                d1 { db }
            }

            [use db]
            model Horse {
                primary {
                    id: int
                }

                name: string
            }
        "#,
        );
        let value = json!({ "id": 1 });
        let result = validate(
            CidlType::Partial {
                object_name: "Horse",
            },
            Some(value),
            &idl,
        );
        assert!(result.is_ok());
        let obj = result.unwrap();
        assert_eq!(obj, Some(json!({ "id": 1 })));
    }

    // partial KV passes
    {
        let idl = src_to_idl(
            r#"
            env {
                kv { namespace otherNamespace }
            }

            model PureKVModel {
                keyfield { id: string }

                kv(namespace, "path/to/data/{id}") { data: json }
                kv(otherNamespace, "path/to/other/{id}") { otherData: string }
            }
            "#,
        );

        let value = json!({
            "id": "test-id",
            "data": { "raw": { "foo": "bar" } },
            "otherData": { "raw": "some string data" }
        });

        let result = validate(
            CidlType::Partial {
                object_name: "PureKVModel",
            },
            Some(value),
            &idl,
        );

        assert!(result.is_ok());
        let obj = result.unwrap().unwrap();
        assert_eq!(
            obj,
            json!({
                "id": "test-id",
                "data": {
                    "key": null,
                    "metadata": null,
                    "raw": { "foo": "bar" }
                },
                "otherData": {
                    "key": null,
                    "metadata": null,
                    "raw": "some string data"
                }
            })
        );
    }
}

#[test]
fn one_to_many_nav_person_dogs() {
    let idl = src_to_idl(
        r#"
        env {
            d1 { db }
        }

        [use db]
        [crud save, get]
        model Person {
            primary { id: int }

            nav (Dog::personId) {
                dogs
            }
        }

        [use db]
        [crud save]
        model Dog {
            primary { id: int }

            foreign (Person::id) {
                personId
                nav { person }
            }
        }
        "#,
    );

    let value = json!({
        "id": 1,
        "dogs": [
            {
                "id": 101,
                "personId": 1
            }
        ]
    });

    let result = validate(
        CidlType::Object { name: "Person" },
        Some(value.clone()),
        &idl,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(value));
}

#[test]
fn json_value_type_accepts_anything() {
    let idl = empty_idl();
    for val in [
        json!(null),
        json!(1),
        json!("text"),
        json!([1, 2]),
        json!({"a": 1}),
    ] {
        let result = validate(CidlType::Json, Some(val.clone()), &idl);
        assert_eq!(result.unwrap(), Some(val));
    }
}

#[test]
fn paginated() {
    // Valid KV paginated result
    {
        let idl = empty_idl();
        let value = json!({
            "results": [
                { "key": "item1", "raw": { "data": "value1" }, "metadata": null },
                { "key": "item2", "raw": { "data": "value2" }, "metadata": { "custom": "field" } }
            ],
            "cursor": "next_page_cursor",
            "complete": false
        });
        let result = validate(
            CidlType::Paginated(Box::new(CidlType::KvObject(Box::new(CidlType::Json)))),
            Some(value),
            &idl,
        );
        assert!(result.is_ok());
        let validated = result.unwrap().unwrap();
        assert_eq!(
            validated.get("cursor").and_then(|v| v.as_str()),
            Some("next_page_cursor")
        );
        assert_eq!(
            validated.get("complete").and_then(|v| v.as_bool()),
            Some(false)
        );
    }

    // Missing cursor defaults to null
    {
        let idl = empty_idl();
        let value = json!({ "results": [], "complete": true });
        let result = validate(
            CidlType::Paginated(Box::new(CidlType::KvObject(Box::new(CidlType::Json)))),
            Some(value),
            &idl,
        );
        assert!(result.is_ok());
        let validated = result.unwrap().unwrap();
        assert!(validated.get("cursor").unwrap().is_null());
    }

    // Invalid cursor type fails
    {
        let idl = empty_idl();
        let value = json!({ "results": [], "cursor": 123, "complete": true });
        let result = validate(
            CidlType::Paginated(Box::new(CidlType::R2Object)),
            Some(value),
            &idl,
        );
        assert!(matches!(result, Err(OrmErrorKind::TypeMismatch { .. })));
    }
}

#[test]
fn validator_less_than() {
    let idl = empty_idl();

    // Int fail: 5 is not < 5
    {
        let result = validate_with(
            CidlType::Int,
            &[Validator::LessThan(Number::Int(5))],
            Some(json!(5)),
            &idl,
        );
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotLessThan {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Int pass: 4 < 5
    {
        let result = validate_with(
            CidlType::Int,
            &[Validator::LessThan(Number::Int(5))],
            Some(json!(4)),
            &idl,
        );
        assert_eq!(result.unwrap(), Some(json!(4)));
    }

    // Float fail: 3.0 is not < 3.0
    {
        let result = validate_with(
            CidlType::Real,
            &[Validator::LessThan(Number::Float(3.0))],
            Some(json!(3.0)),
            &idl,
        );
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotLessThan {
                expected: Number::Float(_),
                ..
            })
        ));
    }

    // Float pass: 2.9 < 3.0
    {
        let result = validate_with(
            CidlType::Real,
            &[Validator::LessThan(Number::Float(3.0))],
            Some(json!(2.9)),
            &idl,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_less_than_or_equal() {
    let idl = empty_idl();

    // Int fail: 6 is not <= 5
    {
        let result = validate_with(
            CidlType::Int,
            &[Validator::LessThanOrEqual(Number::Int(5))],
            Some(json!(6)),
            &idl,
        );
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotLessThanOrEqual {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Int pass: 5 <= 5
    {
        let result = validate_with(
            CidlType::Int,
            &[Validator::LessThanOrEqual(Number::Int(5))],
            Some(json!(5)),
            &idl,
        );
        assert_eq!(result.unwrap(), Some(json!(5)));
    }

    // Float fail: 3.1 is not <= 3.0
    {
        let result = validate_with(
            CidlType::Real,
            &[Validator::LessThanOrEqual(Number::Float(3.0))],
            Some(json!(3.1)),
            &idl,
        );
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotLessThanOrEqual { .. })
        ));
    }

    // Float pass: 3.0 <= 3.0
    {
        let result = validate_with(
            CidlType::Real,
            &[Validator::LessThanOrEqual(Number::Float(3.0))],
            Some(json!(3.0)),
            &idl,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_greater_than() {
    let idl = empty_idl();

    // Int fail: 5 is not > 5
    {
        let result = validate_with(
            CidlType::Int,
            &[Validator::GreaterThan(Number::Int(5))],
            Some(json!(5)),
            &idl,
        );
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotGreaterThan {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Int pass: 6 > 5
    {
        let result = validate_with(
            CidlType::Int,
            &[Validator::GreaterThan(Number::Int(5))],
            Some(json!(6)),
            &idl,
        );
        assert_eq!(result.unwrap(), Some(json!(6)));
    }

    // Float fail: 2.9 is not > 3.0
    {
        let result = validate_with(
            CidlType::Real,
            &[Validator::GreaterThan(Number::Float(3.0))],
            Some(json!(2.9)),
            &idl,
        );
        assert!(matches!(result, Err(OrmErrorKind::NotGreaterThan { .. })));
    }

    // Float pass: 3.1 > 3.0
    {
        let result = validate_with(
            CidlType::Real,
            &[Validator::GreaterThan(Number::Float(3.0))],
            Some(json!(3.1)),
            &idl,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_greater_than_or_equal() {
    let idl = empty_idl();

    // Int fail: 4 is not >= 5
    {
        let result = validate_with(
            CidlType::Int,
            &[Validator::GreaterThanOrEqual(Number::Int(5))],
            Some(json!(4)),
            &idl,
        );
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotGreaterThanOrEqual {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Int pass: 5 >= 5
    {
        let result = validate_with(
            CidlType::Int,
            &[Validator::GreaterThanOrEqual(Number::Int(5))],
            Some(json!(5)),
            &idl,
        );
        assert_eq!(result.unwrap(), Some(json!(5)));
    }

    // Float fail: 2.9 is not >= 3.0
    {
        let result = validate_with(
            CidlType::Real,
            &[Validator::GreaterThanOrEqual(Number::Float(3.0))],
            Some(json!(2.9)),
            &idl,
        );
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotGreaterThanOrEqual { .. })
        ));
    }

    // Float pass: 3.0 >= 3.0
    {
        let result = validate_with(
            CidlType::Real,
            &[Validator::GreaterThanOrEqual(Number::Float(3.0))],
            Some(json!(3.0)),
            &idl,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_step() {
    let idl = empty_idl();

    // Fail: 7 % 5 != 0
    {
        let result = validate_with(CidlType::Int, &[Validator::Step(5)], Some(json!(7)), &idl);
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotStep {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Pass: 15 % 5 == 0
    {
        let result = validate_with(CidlType::Int, &[Validator::Step(5)], Some(json!(15)), &idl);
        assert_eq!(result.unwrap(), Some(json!(15)));
    }
}

#[test]
fn validator_length() {
    let idl = empty_idl();

    // Fail
    {
        let result = validate_with(
            CidlType::String,
            &[Validator::Length(5)],
            Some(json!("hi")),
            &idl,
        );
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotLength {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Pass: length is exactly 5
    {
        let result = validate_with(
            CidlType::String,
            &[Validator::Length(5)],
            Some(json!("hello")),
            &idl,
        );
        assert_eq!(result.unwrap(), Some(json!("hello")));
    }
}

#[test]
fn validator_min_length() {
    let idl = empty_idl();

    // Fail: < 5
    {
        let result = validate_with(
            CidlType::String,
            &[Validator::MinLength(5)],
            Some(json!("hi")),
            &idl,
        );
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotMinLength {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Pass: length >= 5
    {
        let result = validate_with(
            CidlType::String,
            &[Validator::MinLength(5)],
            Some(json!("hello world")),
            &idl,
        );
        assert!(result.is_ok());
    }

    // Pass: exactly min length
    {
        let result = validate_with(
            CidlType::String,
            &[Validator::MinLength(5)],
            Some(json!("hello")),
            &idl,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_max_length() {
    let idl = empty_idl();

    // Fail: length > 5
    {
        let result = validate_with(
            CidlType::String,
            &[Validator::MaxLength(5)],
            Some(json!("hello world")),
            &idl,
        );
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotMaxLength {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Pass: length <= 5
    {
        let result = validate_with(
            CidlType::String,
            &[Validator::MaxLength(5)],
            Some(json!("hi")),
            &idl,
        );
        assert!(result.is_ok());
    }

    // Pass: exactly max length
    {
        let result = validate_with(
            CidlType::String,
            &[Validator::MaxLength(5)],
            Some(json!("hello")),
            &idl,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_regex() {
    let idl = empty_idl();

    // Fail
    {
        let result = validate_with(
            CidlType::String,
            &[Validator::Regex("^[a-z]+$".into())],
            Some(json!("hello123")),
            &idl,
        );
        assert!(matches!(result, Err(OrmErrorKind::UnmatchedRegex { .. })));
    }

    // Pass
    {
        let result = validate_with(
            CidlType::String,
            &[Validator::Regex("^[a-z]+$".into())],
            Some(json!("hello")),
            &idl,
        );
        assert_eq!(result.unwrap(), Some(json!("hello")));
    }
}

#[test]
fn validators_in_model() {
    let idl = src_to_idl(
        r#"
        env {
            d1 { db }
            kv { store }
        }

        [use db]
        model Product {
            primary {
                id: int
            }

            [gt 0]
            price: int

            [minlen 3]
            [maxlen 50]
            name: string

            kv(store, "product/{id}/meta") {
                [regex /^[a-z0-9_]+$/]
                slug: string
            }

            keyfield {
                [maxlen 20]
                kf: string
            }
        }
        "#,
    );

    // Pass
    {
        let value = json!({
            "id": 1,
            "price": 100,
            "name": "Widget",
            "slug": {
                "key": "product/1/meta",
                "metadata": null,
                "raw": "widget_123"
            },
            "kf": "key123"
        });
        let result = validate(
            CidlType::Object { name: "Product" },
            Some(value.clone()),
            &idl,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(value));
    }

    // Fail price <= 0
    {
        let value = json!({
            "id": 1,
            "price": 0,
            "name": "Widget",
            "slug": {
                "key": "product/1/meta",
                "metadata": null,
                "raw": "widget_123"
            },
            "kf": "key123"
        });
        let result = validate(CidlType::Object { name: "Product" }, Some(value), &idl);
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotGreaterThan {
                expected: Number::Int(0),
                ..
            })
        ));
    }

    // Fail name too short
    {
        let value = json!({
            "id": 1,
            "price": 100,
            "name": "Hi",
            "slug": {
                "key": "product/1/meta",
                "metadata": null,
                "raw": "widget_123"
            },
            "kf": "key123"
        });
        let result = validate(CidlType::Object { name: "Product" }, Some(value), &idl);
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotMinLength {
                expected: Number::Int(3),
                ..
            })
        ));
    }

    // Fail slug regex
    {
        let value = json!({
            "id": 1,
            "price": 100,
            "name": "Widget",
            "slug": {
                "key": "product/1/meta",
                "metadata": null,
                "raw": "Invalid Slug!"
            },
            "kf": "key123"
        });
        let result = validate(CidlType::Object { name: "Product" }, Some(value), &idl);
        assert!(matches!(result, Err(OrmErrorKind::UnmatchedRegex { .. })));
    }

    // Fail kf too long
    {
        let value = json!({
            "id": 1,
            "price": 100,
            "name": "Widget",
            "slug": {
                "key": "product/1/meta",
                "metadata": null,
                "raw": "widget_123"
            },
            "kf": "this_key_is_way_too_long"
        });
        let result = validate(CidlType::Object { name: "Product" }, Some(value), &idl);
        assert!(matches!(
            result,
            Err(OrmErrorKind::NotMaxLength {
                expected: Number::Int(20),
                ..
            })
        ));
    }
}
