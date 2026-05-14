mod common;

use compiler_test::src_to_idl;
use orm::upsert::UpsertModel;
use serde_json::{Map, Value, json};
use sqlx::{Row, SqlitePool};

use common::test_sql;

fn include(val: serde_json::Value) -> Option<Map<String, Value>> {
    Some(val.as_object().unwrap().clone())
}

#[sqlx::test]
async fn upsert_scalar_model(db: SqlitePool) {
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
            age: int
        }
    "#,
    );

    let new_model = json!({
        "id": 1,
        "name": "Spirit",
        "age": 5
    });

    let stmts = UpsertModel::query("Horse", &idl, new_model.as_object().unwrap().clone(), None)
        .expect("upsert to succeed");

    let results = test_sql(
        idl,
        stmts.sql.into_iter().map(|r| (r.query, r.values)).collect(),
        db,
    )
    .await
    .expect("SQL to succeed");

    let row = &results[results.len() - 2][0];
    assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
    assert_eq!(row.try_get::<String, _>("name").unwrap(), "Spirit");
    assert_eq!(row.try_get::<i64, _>("age").unwrap(), 5);
}

#[sqlx::test]
async fn upsert_auto_increment(db: SqlitePool) {
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

    // No id provided — should auto-increment
    let new_model = json!({
        "name": "Shadowfax"
    });

    let stmts = UpsertModel::query("Horse", &idl, new_model.as_object().unwrap().clone(), None)
        .expect("upsert to succeed");

    let results = test_sql(
        idl,
        stmts.sql.into_iter().map(|r| (r.query, r.values)).collect(),
        db,
    )
    .await
    .expect("SQL to succeed");

    let row = &results[results.len() - 2][0];
    assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
    assert_eq!(row.try_get::<String, _>("name").unwrap(), "Shadowfax");
}

#[sqlx::test]
async fn upsert_one_to_one(db: SqlitePool) {
    let idl = || {
        src_to_idl(
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

                foreign(Rider::id) {
                    riderId
                    nav {
                        rider
                    }
                }
            }

            [use db]
            model Rider {
                primary {
                    id: int
                }

                nickname: string
            }
        "#,
        )
    };

    let new_model = json!({
        "id": 1,
        "name": "Spirit",
        "riderId": 1,
        "rider": {
            "id": 1,
            "nickname": "Alice"
        }
    });

    let stmts = UpsertModel::query(
        "Horse",
        &idl(),
        new_model.as_object().unwrap().clone(),
        include(json!({ "rider": {} })),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        idl(),
        stmts.sql.into_iter().map(|r| (r.query, r.values)).collect(),
        db,
    )
    .await
    .expect("SQL to succeed");

    let row = &results[results.len() - 2][0];
    assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
    assert_eq!(row.try_get::<String, _>("name").unwrap(), "Spirit");
    assert_eq!(row.try_get::<i64, _>("riderId").unwrap(), 1);
}

#[sqlx::test]
async fn upsert_one_to_many(db: SqlitePool) {
    let idl = || {
        src_to_idl(
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

                nickname: string

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

    let stmts = UpsertModel::query(
        "Horse",
        &idl(),
        new_model.as_object().unwrap().clone(),
        include(json!({ "riders": {} })),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        idl(),
        stmts.sql.into_iter().map(|r| (r.query, r.values)).collect(),
        db,
    )
    .await
    .expect("SQL to succeed");

    // The select returns 2 rows (one per rider via JOIN)
    let select_rows = &results[results.len() - 2];
    assert_eq!(select_rows.len(), 2);
    assert_eq!(select_rows[0].try_get::<i64, _>("id").unwrap(), 1);
}

#[sqlx::test]
async fn upsert_many_to_many(db: SqlitePool) {
    let idl = || {
        src_to_idl(
            r#"
            env {
                d1 { db }
            }

            [use db]
            model Student {
                primary {
                    id: int
                }

                name: string

                nav(Course::id) {
                    courses
                }
            }

            [use db]
            model Course {
                primary {
                    id: int
                }

                title: string

                nav(Student::id) {
                    students
                }
            }
        "#,
        )
    };

    let new_model = json!({
        "id": 1,
        "name": "Alice",
        "courses": [
            { "id": 1, "title": "Math 101", "students": [] },
            { "id": 2, "title": "History 101", "students": [] }
        ]
    });

    let stmts = UpsertModel::query(
        "Student",
        &idl(),
        new_model.as_object().unwrap().clone(),
        include(json!({ "courses": {} })),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        idl(),
        stmts.sql.into_iter().map(|r| (r.query, r.values)).collect(),
        db,
    )
    .await
    .expect("SQL to succeed");

    let select_rows = &results[results.len() - 2];
    assert_eq!(select_rows.len(), 2);
}

#[sqlx::test]
async fn upsert_composite_pk(db: SqlitePool) {
    let idl = src_to_idl(
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

    let new_model = json!({
        "orderId": 1,
        "productId": 101,
        "quantity": 3
    });

    let stmts = UpsertModel::query(
        "OrderItem",
        &idl,
        new_model.as_object().unwrap().clone(),
        None,
    )
    .expect("upsert to succeed");

    let results = test_sql(
        idl,
        stmts.sql.into_iter().map(|r| (r.query, r.values)).collect(),
        db,
    )
    .await
    .expect("SQL to succeed");

    let row = &results[results.len() - 2][0];
    assert_eq!(row.try_get::<i64, _>("orderId").unwrap(), 1);
    assert_eq!(row.try_get::<i64, _>("productId").unwrap(), 101);
    assert_eq!(row.try_get::<i64, _>("quantity").unwrap(), 3);
}
