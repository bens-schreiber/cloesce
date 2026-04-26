use ast::{CidlType, Number, Validator};
use base64::Engine;
use compiler_test::src_to_ast;
use orm::validate::{ValidatorErrorKind, validate_cidl_type};
use serde_json::{Value, json};

fn empty_ast() -> ast::CloesceAst<'static> {
    src_to_ast(
        r#"
        env {
            d1 { db }
        }
        "#,
    )
}

// Helper to validate a non partial type without validators
fn validate_nv(
    cidl_type: CidlType,
    value: Option<Value>,
    ast: &ast::CloesceAst,
) -> Result<Option<Value>, ValidatorErrorKind> {
    validate_cidl_type(cidl_type, &[], value, ast, false)
}

#[test]
fn undefined() {
    // Missing required field
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::String, None, &ast);
        assert!(matches!(result, Err(ValidatorErrorKind::Undefined)));
    }

    // Allowed for partial types
    {
        let ast = src_to_ast(
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
        let result = validate_nv(
            CidlType::Partial {
                object_name: "SomeModel",
            },
            None,
            &ast,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // Arrays return empty array when undefined, even if not partial
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::Array(Box::new(CidlType::Int)), None, &ast);
        assert_eq!(result.unwrap(), Some(Value::Array(vec![])));
    }
}

#[test]
fn null_value() {
    // Null required field
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::String, Some(Value::Null), &ast);
        assert!(matches!(result, Err(ValidatorErrorKind::Null)));
    }

    // Null string for non-nullable type
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::String, Some(json!("null")), &ast);
        assert!(matches!(result, Err(ValidatorErrorKind::Null)));
    }

    // Null allowed for nullable type
    {
        let ast = empty_ast();
        let result = validate_nv(
            CidlType::nullable(CidlType::String),
            Some(Value::Null),
            &ast,
        );
        assert_eq!(result.unwrap(), Some(Value::Null));
    }

    // Null allowed for partial type
    {
        let ast = src_to_ast(
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
        let result = validate_nv(
            CidlType::Partial {
                object_name: "SomeModel",
            },
            Some(Value::Null),
            &ast,
        );
        assert_eq!(result.unwrap(), Some(Value::Null));
    }
}

#[test]
fn integer() {
    // Non integer returns error
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::Int, Some(json!("not_an_int")), &ast);
        assert!(matches!(result, Err(ValidatorErrorKind::NonI64)));
    }

    // Floats return error
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::Int, Some(json!(3.01)), &ast);
        assert!(matches!(result, Err(ValidatorErrorKind::NonI64)));
    }

    // Integers pass
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::Int, Some(json!(42)), &ast);
        assert_eq!(result.unwrap(), Some(json!(42)));
    }
}

#[test]
fn real() {
    // Float passes
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::Real, Some(json!(3.01)), &ast);
        assert_eq!(result.unwrap(), Some(json!(3.01)));
    }

    // Int passes
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::Real, Some(json!(42)), &ast);
        assert_eq!(result.unwrap(), Some(json!(42)));
    }
}

#[test]
fn date_iso() {
    // Non ISO string returns error
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::DateIso, Some(json!("2024-01-15 10:30:00")), &ast);
        assert!(matches!(result, Err(ValidatorErrorKind::NonDateIso)));
    }

    // Valid ISO string with timezone passes
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::DateIso, Some(json!("2024-01-15T10:30:00Z")), &ast);
        assert_eq!(result.unwrap(), Some(json!("2024-01-15T10:30:00Z")));
    }
}

#[test]
fn blob() {
    // Invalid b64 returns error
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::Blob, Some(json!("not valid base64!!!")), &ast);
        assert!(matches!(result, Err(ValidatorErrorKind::NonBase64)));
    }

    // Valid B64 passes
    {
        let ast = empty_ast();
        let encoded = base64::prelude::BASE64_STANDARD.encode(b"hello world");
        let result = validate_nv(CidlType::Blob, Some(json!(encoded)), &ast);
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
        let ast = empty_ast();
        let value = json!({
            "raw": "hello"
        });
        let result = validate_nv(
            CidlType::KvObject(Box::new(CidlType::String)),
            Some(value),
            &ast,
        );
        assert!(matches!(result, Err(ValidatorErrorKind::InvalidKvObject)));
    }

    // Non object metadata
    {
        let ast = empty_ast();
        let value = json!({
            "key": "my-key",
            "raw": "hello",
            "metadata": "not-an-object"
        });
        let result = validate_nv(
            CidlType::KvObject(Box::new(CidlType::String)),
            Some(value),
            &ast,
        );
        assert!(matches!(result, Err(ValidatorErrorKind::InvalidKvObject)));
    }

    // Valid KV passes
    {
        let ast = empty_ast();
        let value = json!({
            "key": "my-key",
            "raw": "hello"
        });
        let result = validate_nv(
            CidlType::KvObject(Box::new(CidlType::String)),
            Some(value),
            &ast,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn array() {
    // Non array returns error
    {
        let ast = empty_ast();
        let result = validate_nv(
            CidlType::array(CidlType::Int),
            Some(json!("not an array")),
            &ast,
        );
        assert!(matches!(result, Err(ValidatorErrorKind::NonArray)));
    }

    // Valid array passes
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::array(CidlType::Int), Some(json!([1, 2, 3])), &ast);
        assert_eq!(result.unwrap(), Some(json!([1, 2, 3])));
    }
}

#[test]
fn r2() {
    // Missing required fields returns error
    {
        let ast = empty_ast();
        let value = json!({
            "key": "some-key"
        });
        let result = validate_nv(CidlType::R2Object, Some(value), &ast);
        assert!(matches!(result, Err(ValidatorErrorKind::InvalidR2Object)));
    }

    // Non object returns error
    {
        let ast = empty_ast();
        let result = validate_nv(CidlType::R2Object, Some(json!("just a string")), &ast);
        assert!(matches!(result, Err(ValidatorErrorKind::InvalidR2Object)));
    }

    // Invalid elements propagate error
    {
        let ast = empty_ast();
        let result = validate_nv(
            CidlType::array(CidlType::Int),
            Some(json!(["not", "integers"])),
            &ast,
        );
        assert!(matches!(result, Err(ValidatorErrorKind::NonI64)));
    }

    // Valid R2 passes
    {
        let ast = empty_ast();
        let value = json!({
            "key": "uploads/photo.jpg",
            "version": "v1",
            "size": 1024,
            "etag": "abc123",
            "http_etag": "\"abc123\"",
            "uploaded": "2024-01-15T10:30:00Z",
            "custom_metadata": null
        });
        let result = validate_nv(CidlType::R2Object, Some(value), &ast);
        assert!(result.is_ok());
    }
}

#[test]
fn data_source() {
    // Unknown data source
    {
        let ast = src_to_ast(
            r#"
            env {
                d1 { db }
            }

            [use db]
            model Horse {
                primary {
                    id: int
                }
            }
        "#,
        );
        let result = validate_nv(
            CidlType::DataSource {
                model_name: "Horse",
            },
            Some(json!("nonexistent_source")),
            &ast,
        );
        assert!(matches!(result, Err(ValidatorErrorKind::UnknownDataSource)));
    }
}

#[test]
fn objects_partials() {
    // Invalid column propagates
    {
        let ast = src_to_ast(
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
        let result = validate_nv(CidlType::Object { name: "Horse" }, Some(value), &ast);
        assert!(matches!(result, Err(ValidatorErrorKind::NonString)));
    }

    // Valid object passes
    {
        let ast = src_to_ast(
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
        let result = validate_nv(
            CidlType::Object { name: "Horse" },
            Some(value.clone()),
            &ast,
        );
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(value));
    }

    // Partial type allows missing fields
    {
        let ast = src_to_ast(
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
        let result = validate_nv(
            CidlType::Partial {
                object_name: "Horse",
            },
            Some(value),
            &ast,
        );
        assert!(result.is_ok());
        let obj = result.unwrap();
        assert_eq!(obj, Some(json!({ "id": 1 })));
    }

    // partial KV passes
    {
        let ast = src_to_ast(
            r#"
            env {
                kv { namespace otherNamespace }
            }

            model PureKVModel {
                keyfield { id }

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

        let result = validate_nv(
            CidlType::Partial {
                object_name: "PureKVModel",
            },
            Some(value),
            &ast,
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
    let ast = src_to_ast(
        r#"
        env {
            d1 { db }
        }

        [use db, save, get]
        model Person {
            primary { id: int }

            nav (Dog::personId) {
                dogs
            }
        }

        [use db, save]
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

    let result = validate_nv(
        CidlType::Object { name: "Person" },
        Some(value.clone()),
        &ast,
    );

    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(value));
}

#[test]
fn json_value_type_accepts_anything() {
    let ast = empty_ast();
    for val in [
        json!(null),
        json!(1),
        json!("text"),
        json!([1, 2]),
        json!({"a": 1}),
    ] {
        let result = validate_nv(CidlType::Json, Some(val.clone()), &ast);
        assert_eq!(result.unwrap(), Some(val));
    }
}

#[test]
fn paginated() {
    // Valid KV paginated result
    {
        let ast = empty_ast();
        let value = json!({
            "results": [
                { "key": "item1", "raw": { "data": "value1" }, "metadata": null },
                { "key": "item2", "raw": { "data": "value2" }, "metadata": { "custom": "field" } }
            ],
            "cursor": "next_page_cursor",
            "complete": false
        });
        let result = validate_nv(
            CidlType::Paginated(Box::new(CidlType::KvObject(Box::new(CidlType::Json)))),
            Some(value),
            &ast,
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
        let ast = empty_ast();
        let value = json!({ "results": [], "complete": true });
        let result = validate_nv(
            CidlType::Paginated(Box::new(CidlType::KvObject(Box::new(CidlType::Json)))),
            Some(value),
            &ast,
        );
        assert!(result.is_ok());
        let validated = result.unwrap().unwrap();
        assert!(validated.get("cursor").unwrap().is_null());
    }

    // Invalid cursor type fails
    {
        let ast = empty_ast();
        let value = json!({ "results": [], "cursor": 123, "complete": true });
        let result = validate_nv(
            CidlType::Paginated(Box::new(CidlType::R2Object)),
            Some(value),
            &ast,
        );
        assert!(matches!(result, Err(ValidatorErrorKind::NonString)));
    }
}

#[test]
fn validator_less_than() {
    let ast = empty_ast();

    // Int fail: 5 is not < 5
    {
        let result = validate_cidl_type(
            CidlType::Int,
            &[Validator::LessThan(Number::Int(5))],
            Some(json!(5)),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotLessThan {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Int pass: 4 < 5
    {
        let result = validate_cidl_type(
            CidlType::Int,
            &[Validator::LessThan(Number::Int(5))],
            Some(json!(4)),
            &ast,
            false,
        );
        assert_eq!(result.unwrap(), Some(json!(4)));
    }

    // Float fail: 3.0 is not < 3.0
    {
        let result = validate_cidl_type(
            CidlType::Real,
            &[Validator::LessThan(Number::Float(3.0))],
            Some(json!(3.0)),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotLessThan {
                expected: Number::Float(_),
                ..
            })
        ));
    }

    // Float pass: 2.9 < 3.0
    {
        let result = validate_cidl_type(
            CidlType::Real,
            &[Validator::LessThan(Number::Float(3.0))],
            Some(json!(2.9)),
            &ast,
            false,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_less_than_or_equal() {
    let ast = empty_ast();

    // Int fail: 6 is not <= 5
    {
        let result = validate_cidl_type(
            CidlType::Int,
            &[Validator::LessThanOrEqual(Number::Int(5))],
            Some(json!(6)),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotLessThanOrEqual {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Int pass: 5 <= 5
    {
        let result = validate_cidl_type(
            CidlType::Int,
            &[Validator::LessThanOrEqual(Number::Int(5))],
            Some(json!(5)),
            &ast,
            false,
        );
        assert_eq!(result.unwrap(), Some(json!(5)));
    }

    // Float fail: 3.1 is not <= 3.0
    {
        let result = validate_cidl_type(
            CidlType::Real,
            &[Validator::LessThanOrEqual(Number::Float(3.0))],
            Some(json!(3.1)),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotLessThanOrEqual { .. })
        ));
    }

    // Float pass: 3.0 <= 3.0
    {
        let result = validate_cidl_type(
            CidlType::Real,
            &[Validator::LessThanOrEqual(Number::Float(3.0))],
            Some(json!(3.0)),
            &ast,
            false,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_greater_than() {
    let ast = empty_ast();

    // Int fail: 5 is not > 5
    {
        let result = validate_cidl_type(
            CidlType::Int,
            &[Validator::GreaterThan(Number::Int(5))],
            Some(json!(5)),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotGreaterThan {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Int pass: 6 > 5
    {
        let result = validate_cidl_type(
            CidlType::Int,
            &[Validator::GreaterThan(Number::Int(5))],
            Some(json!(6)),
            &ast,
            false,
        );
        assert_eq!(result.unwrap(), Some(json!(6)));
    }

    // Float fail: 2.9 is not > 3.0
    {
        let result = validate_cidl_type(
            CidlType::Real,
            &[Validator::GreaterThan(Number::Float(3.0))],
            Some(json!(2.9)),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotGreaterThan { .. })
        ));
    }

    // Float pass: 3.1 > 3.0
    {
        let result = validate_cidl_type(
            CidlType::Real,
            &[Validator::GreaterThan(Number::Float(3.0))],
            Some(json!(3.1)),
            &ast,
            false,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_greater_than_or_equal() {
    let ast = empty_ast();

    // Int fail: 4 is not >= 5
    {
        let result = validate_cidl_type(
            CidlType::Int,
            &[Validator::GreaterThanOrEqual(Number::Int(5))],
            Some(json!(4)),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotGreaterThanOrEqual {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Int pass: 5 >= 5
    {
        let result = validate_cidl_type(
            CidlType::Int,
            &[Validator::GreaterThanOrEqual(Number::Int(5))],
            Some(json!(5)),
            &ast,
            false,
        );
        assert_eq!(result.unwrap(), Some(json!(5)));
    }

    // Float fail: 2.9 is not >= 3.0
    {
        let result = validate_cidl_type(
            CidlType::Real,
            &[Validator::GreaterThanOrEqual(Number::Float(3.0))],
            Some(json!(2.9)),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotGreaterThanOrEqual { .. })
        ));
    }

    // Float pass: 3.0 >= 3.0
    {
        let result = validate_cidl_type(
            CidlType::Real,
            &[Validator::GreaterThanOrEqual(Number::Float(3.0))],
            Some(json!(3.0)),
            &ast,
            false,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_step() {
    let ast = empty_ast();

    // Fail: 7 % 5 != 0
    {
        let result = validate_cidl_type(
            CidlType::Int,
            &[Validator::Step(5)],
            Some(json!(7)),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotStep {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Pass: 15 % 5 == 0
    {
        let result = validate_cidl_type(
            CidlType::Int,
            &[Validator::Step(5)],
            Some(json!(15)),
            &ast,
            false,
        );
        assert_eq!(result.unwrap(), Some(json!(15)));
    }
}

#[test]
fn validator_length() {
    let ast = empty_ast();

    // Fail
    {
        let result = validate_cidl_type(
            CidlType::String,
            &[Validator::Length(5)],
            Some(json!("hi")),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotLength {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Pass: length is exactly 5
    {
        let result = validate_cidl_type(
            CidlType::String,
            &[Validator::Length(5)],
            Some(json!("hello")),
            &ast,
            false,
        );
        assert_eq!(result.unwrap(), Some(json!("hello")));
    }
}

#[test]
fn validator_min_length() {
    let ast = empty_ast();

    // Fail: < 5
    {
        let result = validate_cidl_type(
            CidlType::String,
            &[Validator::MinLength(5)],
            Some(json!("hi")),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotMinLength {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Pass: length >= 5
    {
        let result = validate_cidl_type(
            CidlType::String,
            &[Validator::MinLength(5)],
            Some(json!("hello world")),
            &ast,
            false,
        );
        assert!(result.is_ok());
    }

    // Pass: exactly min length
    {
        let result = validate_cidl_type(
            CidlType::String,
            &[Validator::MinLength(5)],
            Some(json!("hello")),
            &ast,
            false,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_max_length() {
    let ast = empty_ast();

    // Fail: length > 5
    {
        let result = validate_cidl_type(
            CidlType::String,
            &[Validator::MaxLength(5)],
            Some(json!("hello world")),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotMaxLength {
                expected: Number::Int(5),
                ..
            })
        ));
    }

    // Pass: length <= 5
    {
        let result = validate_cidl_type(
            CidlType::String,
            &[Validator::MaxLength(5)],
            Some(json!("hi")),
            &ast,
            false,
        );
        assert!(result.is_ok());
    }

    // Pass: exactly max length
    {
        let result = validate_cidl_type(
            CidlType::String,
            &[Validator::MaxLength(5)],
            Some(json!("hello")),
            &ast,
            false,
        );
        assert!(result.is_ok());
    }
}

#[test]
fn validator_regex() {
    let ast = empty_ast();

    // Fail
    {
        let result = validate_cidl_type(
            CidlType::String,
            &[Validator::Regex("^[a-z]+$".into())],
            Some(json!("hello123")),
            &ast,
            false,
        );
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::UnmatchedRegex { .. })
        ));
    }

    // Pass
    {
        let result = validate_cidl_type(
            CidlType::String,
            &[Validator::Regex("^[a-z]+$".into())],
            Some(json!("hello")),
            &ast,
            false,
        );
        assert_eq!(result.unwrap(), Some(json!("hello")));
    }
}

#[test]
fn validators_in_model() {
    let ast = src_to_ast(
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
                kf
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
        let result = validate_nv(
            CidlType::Object { name: "Product" },
            Some(value.clone()),
            &ast,
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
        let result = validate_nv(CidlType::Object { name: "Product" }, Some(value), &ast);
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotGreaterThan {
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
        let result = validate_nv(CidlType::Object { name: "Product" }, Some(value), &ast);
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotMinLength {
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
        let result = validate_nv(CidlType::Object { name: "Product" }, Some(value), &ast);
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::UnmatchedRegex { .. })
        ));
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
        let result = validate_nv(CidlType::Object { name: "Product" }, Some(value), &ast);
        assert!(matches!(
            result,
            Err(ValidatorErrorKind::NotMaxLength {
                expected: Number::Int(20),
                ..
            })
        ));
    }
}
