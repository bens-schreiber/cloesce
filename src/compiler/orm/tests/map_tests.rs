mod common;

use compiler_test::src_to_idl;
use orm::map::map_sql;
use orm::upsert::UpsertModel;
use serde_json::{Map, Value, json};
use sqlx::{Column, Row, SqlitePool, TypeInfo, sqlite::SqliteRow};

use common::test_sql;

fn include(val: serde_json::Value) -> Option<Map<String, Value>> {
    Some(val.as_object().unwrap().clone())
}

pub fn rows_to_json(rows: &[SqliteRow]) -> Vec<Map<String, Value>> {
    rows.iter()
        .map(|row| {
            let mut map = Map::new();
            for col in row.columns() {
                let name = col.name().to_string();
                let type_info = col.type_info().name();
                let value = match type_info {
                    "TEXT" => row
                        .try_get::<String, _>(name.as_str())
                        .map(Value::String)
                        .unwrap_or(Value::Null),
                    "INTEGER" => row
                        .try_get::<i64, _>(name.as_str())
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    "REAL" => row
                        .try_get::<f64, _>(name.as_str())
                        .map(Value::from)
                        .unwrap_or(Value::Null),
                    _ => Value::Null,
                };
                map.insert(name, value);
            }
            map
        })
        .collect()
}

#[test]
fn no_records_returns_empty() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Horse for db {
            primary {
                id: int
            }

            column {
                name: option<string>
            }

            nav(Rider::horseId) {
                riders
            }
        }

        model Rider for db {
            primary {
                id: int
            }

            column {
                nickname: option<string>
            }

            foreign(Horse::id) {
                horseId
            }
        }
    "#,
    );
    let rows: Vec<Map<String, Value>> = vec![];
    let result = map_sql("Horse", rows, None, &idl).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn flat() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Horse for db {
            primary {
                id: int
            }

            column {
                name: option<string>
            }
        }
    "#,
    );
    let row = vec![
        ("id".to_string(), json!("1")),
        ("name".to_string(), json!("Lightning")),
    ]
    .into_iter()
    .collect::<Map<String, Value>>();

    let result = map_sql("Horse", vec![row], None, &idl).unwrap();
    let horse = result.first().unwrap().as_object().unwrap();
    assert_eq!(horse.get("id"), Some(&json!("1")));
    assert_eq!(horse.get("name"), Some(&json!("Lightning")));
}

#[test]
fn one_to_one_worker_model_is_skipped() {
    // Arrange
    let idl = src_to_idl(
        r#"
        d1 { db }

        model Person for db {
            primary {
                id: int
            }

            column {
                name: string
            }

            nav Profile::ownerId(id) {
                profile
            }
        }

        model Profile {
            route {
                ownerId: int
            }
        }
    "#,
    );

    let row = vec![
        ("id".to_string(), json!(1)),
        ("name".to_string(), json!("Alice")),
    ]
    .into_iter()
    .collect::<Map<String, Value>>();

    // Act
    let result = map_sql("Person", vec![row], include(json!({ "profile": {} })), &idl)
        .expect("map_sql to succeed");

    // Assert
    let person = result.first().unwrap().as_object().unwrap();
    assert_eq!(person.get("id"), Some(&json!(1)));
    assert_eq!(person.get("name"), Some(&json!("Alice")));
    assert_eq!(person.get("profile"), None);
}

#[sqlx::test]
async fn one_to_one(db: SqlitePool) {
    let idl = || {
        src_to_idl(
            r#"
            d1 { db }

            model Horse for db {
                primary {
                    id: int
                }

                column {
                    name: option<string>
                }

                foreign(Rider::id) {
                    bestRiderId
                }

                nav Rider::id(bestRiderId) {
                    bestRider
                }
            }

            model Rider for db {
                primary {
                    id: int
                }

                column {
                    nickname: option<string>
                }
            }
        "#,
        )
    };

    let new_model = json!({
        "id": 1,
        "name": "Shadowfax",
        "bestRiderId": 1,
        "bestRider": {
            "id": 1,
            "nickname": "Gandalf"
        }
    });

    let include_tree_json = json!({ "bestRider": {} });

    let upsert_res = UpsertModel::query(
        "Horse",
        &idl(),
        new_model.as_object().unwrap().clone(),
        include(include_tree_json.clone()),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        idl(),
        upsert_res
            .sql
            .into_iter()
            .map(|r| (r.query, r.values))
            .collect(),
        db,
    )
    .await
    .expect("test_sql to succeed");

    let select_rows = rows_to_json(results.get(results.len() - 2).unwrap());

    let result = map_sql("Horse", select_rows, include(include_tree_json), &idl())
        .expect("map_sql to succeed");

    assert_eq!(result, vec![new_model]);
}

#[sqlx::test]
async fn one_to_many(db: SqlitePool) {
    let idl = || {
        src_to_idl(
            r#"
            d1 { db }

            model Horse for db {
                primary {
                    id: int
                }

                column {
                    name: option<string>
                }

                nav(Rider::horseId) {
                    riders
                }
            }

            model Rider for db {
                primary {
                    id: int
                }

                column {
                    nickname: option<string>
                }

                foreign(Horse::id) {
                    horseId
                }
            }
        "#,
        )
    };

    let new_model = json!({
        "id": 1,
        "name": "Black Beauty",
        "riders": [
            { "id": 1, "nickname": "Alice", "horseId": 1 },
            { "id": 2, "nickname": "Bob", "horseId": 1 }
        ]
    });

    let include_tree_json = json!({ "riders": {} });

    let upsert_stmts = UpsertModel::query(
        "Horse",
        &idl(),
        new_model.as_object().unwrap().clone(),
        include(include_tree_json.clone()),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        idl(),
        upsert_stmts
            .sql
            .into_iter()
            .map(|r| (r.query, r.values))
            .collect(),
        db,
    )
    .await
    .expect("test_sql to succeed");

    let select_rows = rows_to_json(results.get(results.len() - 2).unwrap());

    let result = map_sql("Horse", select_rows, include(include_tree_json), &idl())
        .expect("map_sql to succeed");

    assert_eq!(result, vec![new_model]);
}

#[test]
fn composite_primary_key_deduplication() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        model OrderItem for db {
            primary {
                orderId: int
                productId: int
            }

            column {
                quantity: int
            }
        }
    "#,
    );

    let rows = vec![
        vec![
            ("orderId".to_string(), json!(1)),
            ("productId".to_string(), json!(101)),
            ("quantity".to_string(), json!(2)),
        ]
        .into_iter()
        .collect::<Map<String, Value>>(),
        vec![
            ("orderId".to_string(), json!(1)),
            ("productId".to_string(), json!(101)),
            ("quantity".to_string(), json!(2)),
        ]
        .into_iter()
        .collect::<Map<String, Value>>(),
    ];

    let result = map_sql("OrderItem", rows, None, &idl).unwrap();
    assert_eq!(result.len(), 1);
    let item = result.first().unwrap().as_object().unwrap();
    assert_eq!(item.get("orderId"), Some(&json!(1)));
    assert_eq!(item.get("productId"), Some(&json!(101)));
    assert_eq!(item.get("quantity"), Some(&json!(2)));
}
