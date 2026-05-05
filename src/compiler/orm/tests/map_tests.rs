mod common;

use compiler_test::src_to_ast;
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

            name: option<string>

            nav(Rider::horseId) {
                riders
            }
        }

        [use db]
        model Rider {
            primary {
                id: int
            }

            nickname: option<string>

            foreign(Horse::id) {
                horseId
            }
        }
    "#,
    );
    let rows: Vec<Map<String, Value>> = vec![];
    let result = map_sql("Horse", rows, None, &ast).unwrap();
    assert_eq!(result.len(), 0);
}

#[test]
fn flat() {
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

            name: option<string>
        }
    "#,
    );
    let row = vec![
        ("id".to_string(), json!("1")),
        ("name".to_string(), json!("Lightning")),
    ]
    .into_iter()
    .collect::<Map<String, Value>>();

    let result = map_sql("Horse", vec![row], None, &ast).unwrap();
    let horse = result.first().unwrap().as_object().unwrap();
    assert_eq!(horse.get("id"), Some(&json!("1")));
    assert_eq!(horse.get("name"), Some(&json!("Lightning")));
}

#[sqlx::test]
async fn one_to_one(db: SqlitePool) {
    let ast = || {
        src_to_ast(
            r#"
            env {
                d1 { db }
            }

            [use db]
            model Horse {
                primary {
                    id: int
                }

                name: option<string>

                foreign(Rider::id) {
                    bestRiderId
                    nav {
                        bestRider
                    }
                }
            }

            [use db]
            model Rider {
                primary {
                    id: int
                }

                nickname: option<string>
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
        &ast(),
        new_model.as_object().unwrap().clone(),
        include(include_tree_json.clone()),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        ast(),
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

    let result = map_sql("Horse", select_rows, include(include_tree_json), &ast())
        .expect("map_sql to succeed");

    assert_eq!(result, vec![new_model]);
}

#[sqlx::test]
async fn one_to_many(db: SqlitePool) {
    let ast = || {
        src_to_ast(
            r#"
            env {
                d1 { db }
            }

            [use db]
            model Horse {
                primary {
                    id: int
                }

                name: option<string>

                nav(Rider::horseId) {
                    riders
                }
            }

            [use db]
            model Rider {
                primary {
                    id: int
                }

                nickname: option<string>

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
        &ast(),
        new_model.as_object().unwrap().clone(),
        include(include_tree_json.clone()),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        ast(),
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

    let result = map_sql("Horse", select_rows, include(include_tree_json), &ast())
        .expect("map_sql to succeed");

    assert_eq!(result, vec![new_model]);
}

#[sqlx::test]
async fn many_to_many(db: SqlitePool) {
    let meta = || {
        src_to_ast(
            r#"
            env {
                d1 { db }
            }

            [use db]
            model Student {
                primary {
                    id: int
                }

                name: option<string>

                nav(Course::id) {
                    courses
                }
            }

            [use db]
            model Course {
                primary {
                    id: int
                }

                title: option<string>

                nav(Student::id) {
                    students
                }
            }
        "#,
        )
    };

    let new_model = json!({
        "id": 1,
        "name": "John Doe",
        "courses": [
            { "id": 1, "title": "Math 101", "students": [] },
            { "id": 2, "title": "History 201", "students": [] }
        ]
    });

    let include_tree_json = json!({ "courses": {} });

    let upsert_stmts = UpsertModel::query(
        "Student",
        &meta(),
        new_model.as_object().unwrap().clone(),
        include(include_tree_json.clone()),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        meta(),
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

    let result = map_sql("Student", select_rows, include(include_tree_json), &meta())
        .expect("map_sql to succeed");

    assert_eq!(result, vec![new_model]);
}

#[test]
fn composite_primary_key_deduplication() {
    let ast = src_to_ast(
        r#"
        env {
            d1 { db }
        }

        [use db]
        model OrderItem {
            primary {
                orderId: int
                productId: int
            }

            quantity: int
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

    let result = map_sql("OrderItem", rows, None, &ast).unwrap();
    assert_eq!(result.len(), 1);
    let item = result.first().unwrap().as_object().unwrap();
    assert_eq!(item.get("orderId"), Some(&json!(1)));
    assert_eq!(item.get("productId"), Some(&json!(101)));
    assert_eq!(item.get("quantity"), Some(&json!(2)));
}
