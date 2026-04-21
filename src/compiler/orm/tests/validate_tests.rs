use ast::CidlType;
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

#[test]
fn undefined() {
    // Missing required field
    {
        let ast = empty_ast();
        let result = validate_cidl_type(CidlType::String, None, &ast, false);
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
        let result = validate_cidl_type(
            CidlType::Partial {
                object_name: "SomeModel",
            },
            None,
            &ast,
            false,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // Arrays return empty array when undefined, even if not partial
    {
        let ast = empty_ast();
        let result =
            validate_cidl_type(CidlType::Array(Box::new(CidlType::Int)), None, &ast, false);
        assert_eq!(result.unwrap(), Some(Value::Array(vec![])));
    }
}

#[test]
fn null_value() {
    // Null required field
    {
        let ast = empty_ast();
        let result = validate_cidl_type(CidlType::String, Some(Value::Null), &ast, false);
        assert!(matches!(result, Err(ValidatorErrorKind::Null)));
    }

    // Null string for non-nullable type
    {
        let ast = empty_ast();
        let result = validate_cidl_type(CidlType::String, Some(json!("null")), &ast, false);
        assert!(matches!(result, Err(ValidatorErrorKind::Null)));
    }

    // Null allowed for nullable type
    {
        let ast = empty_ast();
        let result = validate_cidl_type(
            CidlType::nullable(CidlType::String),
            Some(Value::Null),
            &ast,
            false,
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
        let result = validate_cidl_type(
            CidlType::Partial {
                object_name: "SomeModel",
            },
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
        let ast = empty_ast();
        let result = validate_cidl_type(CidlType::Int, Some(json!("not_an_int")), &ast, false);
        assert!(matches!(result, Err(ValidatorErrorKind::NonI64)));
    }

    // Floats return error
    {
        let ast = empty_ast();
        let result = validate_cidl_type(CidlType::Int, Some(json!(3.01)), &ast, false);
        assert!(matches!(result, Err(ValidatorErrorKind::NonI64)));
    }

    // Integers pass
    {
        let ast = empty_ast();
        let result = validate_cidl_type(CidlType::Int, Some(json!(42)), &ast, false);
        assert_eq!(result.unwrap(), Some(json!(42)));
    }
}

#[test]
fn real() {
    // Float passes
    {
        let ast = empty_ast();
        let result = validate_cidl_type(CidlType::Real, Some(json!(3.01)), &ast, false);
        assert_eq!(result.unwrap(), Some(json!(3.01)));
    }

    // Int passes
    {
        let ast = empty_ast();
        let result = validate_cidl_type(CidlType::Real, Some(json!(42)), &ast, false);
        assert_eq!(result.unwrap(), Some(json!(42)));
    }
}

#[test]
fn date_iso() {
    // Non ISO string returns error
    {
        let ast = empty_ast();
        let result = validate_cidl_type(
            CidlType::DateIso,
            Some(json!("2024-01-15 10:30:00")),
            &ast,
            false,
        );
        assert!(matches!(result, Err(ValidatorErrorKind::NonDateIso)));
    }

    // Valid ISO string with timezone passes
    {
        let ast = empty_ast();
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
        let ast = empty_ast();
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
        let ast = empty_ast();
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
        let ast = empty_ast();
        let value = json!({
            "raw": "hello"
        });
        let result = validate_cidl_type(
            CidlType::KvObject(Box::new(CidlType::String)),
            Some(value),
            &ast,
            false,
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
        let result = validate_cidl_type(
            CidlType::KvObject(Box::new(CidlType::String)),
            Some(value),
            &ast,
            false,
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
        let result = validate_cidl_type(
            CidlType::KvObject(Box::new(CidlType::String)),
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
        let ast = empty_ast();
        let result = validate_cidl_type(
            CidlType::array(CidlType::Int),
            Some(json!("not an array")),
            &ast,
            false,
        );
        assert!(matches!(result, Err(ValidatorErrorKind::NonArray)));
    }

    // Valid array passes
    {
        let ast = empty_ast();
        let result = validate_cidl_type(
            CidlType::array(CidlType::Int),
            Some(json!([1, 2, 3])),
            &ast,
            false,
        );
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
        let result = validate_cidl_type(CidlType::R2Object, Some(value), &ast, false);
        assert!(matches!(result, Err(ValidatorErrorKind::InvalidR2Object)));
    }

    // Non object returns error
    {
        let ast = empty_ast();
        let result = validate_cidl_type(
            CidlType::R2Object,
            Some(json!("just a string")),
            &ast,
            false,
        );
        assert!(matches!(result, Err(ValidatorErrorKind::InvalidR2Object)));
    }

    // Invalid elements propagate error
    {
        let ast = empty_ast();
        let result = validate_cidl_type(
            CidlType::array(CidlType::Int),
            Some(json!(["not", "integers"])),
            &ast,
            false,
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
        let result = validate_cidl_type(CidlType::R2Object, Some(value), &ast, false);
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
        let result = validate_cidl_type(
            CidlType::DataSource {
                model_name: "Horse",
            },
            Some(json!("nonexistent_source")),
            &ast,
            false,
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
        let result =
            validate_cidl_type(CidlType::Object { name: "Horse" }, Some(value), &ast, false);
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
        let result = validate_cidl_type(
            CidlType::Object { name: "Horse" },
            Some(value.clone()),
            &ast,
            false,
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
        let result = validate_cidl_type(
            CidlType::Partial {
                object_name: "Horse",
            },
            Some(value),
            &ast,
            false,
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

        let result = validate_cidl_type(
            CidlType::Partial {
                object_name: "PureKVModel",
            },
            Some(value),
            &ast,
            false,
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

    let result = validate_cidl_type(
        CidlType::Object { name: "Person" },
        Some(value.clone()),
        &ast,
        false,
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
        let result = validate_cidl_type(CidlType::Json, Some(val.clone()), &ast, false);
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
        let result = validate_cidl_type(
            CidlType::Paginated(Box::new(CidlType::KvObject(Box::new(CidlType::Json)))),
            Some(value),
            &ast,
            false,
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
        let result = validate_cidl_type(
            CidlType::Paginated(Box::new(CidlType::KvObject(Box::new(CidlType::Json)))),
            Some(value),
            &ast,
            false,
        );
        assert!(result.is_ok());
        let validated = result.unwrap().unwrap();
        assert!(validated.get("cursor").unwrap().is_null());
    }

    // Invalid cursor type fails
    {
        let ast = empty_ast();
        let value = json!({ "results": [], "cursor": 123, "complete": true });
        let result = validate_cidl_type(
            CidlType::Paginated(Box::new(CidlType::R2Object)),
            Some(value),
            &ast,
            false,
        );
        assert!(matches!(result, Err(ValidatorErrorKind::NonString)));
    }
}
