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
        d1 { db }

        model Horse for db {
            primary {
                id: int
            }

            column {
                name: string
                age: int
            }
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
        d1 { db }

        model Horse for db {
            primary {
                id: int
            }

            column {
                name: string
            }
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
            d1 { db }

            model Horse for db {
                primary {
                    id: int
                }

                column {
                    name: string
                }

                foreign(Rider::id) {
                    riderId
                }

                nav Rider::id(riderId) {
                    rider
                }
            }

            model Rider for db {
                primary {
                    id: int
                }

                column {
                    nickname: string
                }
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
            d1 { db }

            model Horse for db {
                primary {
                    id: int
                }

                column {
                    name: string
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
                    nickname: string
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
async fn upsert_composite_pk(db: SqlitePool) {
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

#[sqlx::test]
async fn upsert_join_table_composite_fk_pk(db: SqlitePool) {
    // Arrange
    let idl = || {
        src_to_idl(
            r#"
            d1 { db }

            model Hamburger for db {
                primary {
                    id: int
                }

                column {
                    name: string
                }

                nav(HamburgerTopping::hamburgerId) {
                    toppings
                }
            }

            model Topping for db {
                primary {
                    id: int
                }

                column {
                    name: string
                }
            }

            model HamburgerTopping for db {
                primary {
                    foreign(Hamburger::id) {
                        hamburgerId
                    }

                    foreign(Topping::id) {
                        toppingId
                    }
                }

                nav Topping::id(toppingId) {
                    topping
                }
            }
        "#,
        )
    };

    let new_model = json!({
        "id": 1,
        "name": "bacon lettuce burger",
        "toppings": [
            { "topping": { "id": 101, "name": "bacon" } },
            { "topping": { "id": 102, "name": "lettuce" } }
        ]
    });

    // Act
    let stmts = UpsertModel::query(
        "Hamburger",
        &idl(),
        new_model.as_object().unwrap().clone(),
        include(json!({ "toppings": { "topping": {} } })),
    )
    .expect("upsert to succeed");

    // Assert
    let results = test_sql(
        idl(),
        stmts.sql.into_iter().map(|r| (r.query, r.values)).collect(),
        db,
    )
    .await
    .expect("SQL to succeed");

    let select_rows = &results[results.len() - 2];
    assert_eq!(select_rows.len(), 2);

    let mut topping_ids: Vec<i64> = select_rows
        .iter()
        .map(|r| {
            assert_eq!(r.try_get::<i64, _>("id").unwrap(), 1);
            assert_eq!(r.try_get::<i64, _>("toppings.hamburgerId").unwrap(), 1);
            r.try_get::<i64, _>("toppings.toppingId").unwrap()
        })
        .collect();
    topping_ids.sort();
    assert_eq!(topping_ids, vec![101, 102]);
}

#[test]
fn upsert_route_model_persists_kv_without_sql() {
    let idl = src_to_idl(
        r#"
        d1 { db }

        kv namespace {
            data(id: int) -> json {
                "data/{id}"
            }
        }

        kv otherNamespace {
            otherData(siblingId: int) -> json {
                "other/{siblingId}"
            }
        }

        model RouteOwner {
            route {
                id: int
            }

            kv namespace::data(id) {
                someData
            }

            nav RouteSibling::siblingId(id) {
                sibling
            }
        }

        model RouteSibling {
            route {
                siblingId: int
            }

            kv otherNamespace::otherData(siblingId) {
                siblingData
            }
        }
    "#,
    );

    let new_model = json!({
        "id": 7,
        "someData": { "raw": { "hello": "world" } },
        "sibling": {
            "siblingId": 7,
            "siblingData": { "raw": { "foo": "bar" } }
        }
    });

    let res = UpsertModel::query(
        "RouteOwner",
        &idl,
        new_model.as_object().unwrap().clone(),
        include(json!({ "someData": {}, "sibling": { "siblingData": {} } })),
    )
    .expect("upsert to succeed");

    // Route models have no SQL representation.
    assert!(res.sql.is_empty());

    // The model's own KV field and its nav target's KV field are both persisted.
    let keys: Vec<&str> = res.kv_uploads.iter().map(|u| u.key.as_str()).collect();
    assert!(keys.contains(&"data/7"), "got keys: {keys:?}");
    assert!(keys.contains(&"other/7"), "got keys: {keys:?}");
}
