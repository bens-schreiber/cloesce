mod common;

use compiler_test::src_to_ast;
use orm::upsert::UpsertModel;
use serde_json::{Map, Value, json};
use sqlx::{Row, SqlitePool};

use common::test_sql;

fn include(val: serde_json::Value) -> Option<Map<String, Value>> {
    Some(val.as_object().unwrap().clone())
}

#[sqlx::test]
async fn upsert_scalar_model(db: SqlitePool) {
    let ast = src_to_ast(
        r#"
        env { db: d1 }
        @d1(db) model Horse {
            [primary id]
            id: int
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

    let stmts = UpsertModel::query("Horse", &ast, new_model.as_object().unwrap().clone(), None)
        .expect("upsert to succeed");

    let results = test_sql(
        ast,
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
    let ast = src_to_ast(
        r#"
        env { db: d1 }
        @d1(db) model Horse {
            [primary id]
            id: int
            name: string
        }
    "#,
    );

    // No id provided — should auto-increment
    let new_model = json!({
        "name": "Shadowfax"
    });

    let stmts = UpsertModel::query("Horse", &ast, new_model.as_object().unwrap().clone(), None)
        .expect("upsert to succeed");

    let results = test_sql(
        ast,
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
    let ast = || {
        src_to_ast(
            r#"
            env { db: d1 }
            @d1(db) model Horse {
                [primary id]
                id: int
                name: string

                [foreign riderId -> Rider::id]
                [nav rider -> riderId]
                riderId: int
                rider: Rider
            }
            @d1(db) model Rider {
                [primary id]
                id: int
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
        &ast(),
        new_model.as_object().unwrap().clone(),
        include(json!({ "rider": {} })),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        ast(),
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
    let ast = || {
        src_to_ast(
            r#"
            env { db: d1 }
            @d1(db) model Horse {
                [primary id]
                id: int
                name: string

                [nav riders -> Rider::horseId]
                riders: Array<Rider>
            }
            @d1(db) model Rider {
                [primary id]
                id: int
                nickname: string

                [foreign horseId -> Horse::id]
                horseId: int
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
        &ast(),
        new_model.as_object().unwrap().clone(),
        include(json!({ "riders": {} })),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        ast(),
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
    let ast = || {
        src_to_ast(
            r#"
            env { db: d1 }
            @d1(db) model Student {
                [primary id]
                id: int
                name: string

                [nav courses <> Course::students]
                courses: Array<Course>
            }
            @d1(db) model Course {
                [primary id]
                id: int
                title: string

                [nav students <> Student::courses]
                students: Array<Student>
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
        &ast(),
        new_model.as_object().unwrap().clone(),
        include(json!({ "courses": {} })),
    )
    .expect("upsert to succeed");

    let results = test_sql(
        ast(),
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
    let ast = src_to_ast(
        r#"
        env { db: d1 }
        @d1(db) model OrderItem {
            [primary orderId, productId]
            orderId: int
            productId: int
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
        &ast,
        new_model.as_object().unwrap().clone(),
        None,
    )
    .expect("upsert to succeed");

    let results = test_sql(
        ast,
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
