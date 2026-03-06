use ast::CloesceAst;
use ast::Model;
use ast::NavigationPropertyKind;
use ast::fail;
use indexmap::IndexMap;
use serde_json::Map;
use serde_json::Value;

use crate::D1Result;
use crate::IncludeTreeJson;
use crate::methods::OrmErrorKind;

use super::Result;

pub fn map_sql(
    model_name: &str,
    rows: D1Result,
    include_tree: Option<IncludeTreeJson>,
    ast: &CloesceAst,
) -> Result<Vec<Value>> {
    let model = match ast.models.get(model_name) {
        Some(m) => m,
        None => fail!(OrmErrorKind::UnknownModel, "{}", model_name),
    };
    if !model.has_d1() {
        fail!(
            OrmErrorKind::ModelMissingD1,
            "Model {} is not a D1 model",
            model_name
        )
    }

    let mut result_map = IndexMap::new();

    // Scan each row for the root model (`model_name`)'s primary key
    for row in rows.iter() {
        // Build a composite key from all primary key columns
        let mut pk_values = Vec::new();
        let mut all_pks_present = true;
        for pk_col in &model.primary_key_columns {
            let pk_name = &pk_col.value.name;
            if let Some(pk_value) = row.get(pk_name) {
                pk_values.push((pk_name.clone(), pk_value.clone()));
            } else {
                // One or more primary key columns are missing
                all_pks_present = false;
                break;
            }
        }

        if !all_pks_present {
            // The root model's primary key is not fully present in the row
            continue;
        }

        // Create a composite key for the result map
        let composite_key = Value::Array(pk_values.iter().map(|(_, v)| v.clone()).collect());

        // A particular primary key will only exist once. If that key does not yet
        // exist, put a new model into the result map.
        let model_json = result_map.entry(composite_key).or_insert_with(|| {
            let mut m = serde_json::Map::new();

            // Set primary key columns
            for (pk_name, pk_value) in &pk_values {
                m.insert(pk_name.clone(), pk_value.clone());
            }

            // Set scalar columns
            for col in &model.columns {
                let attr_name = &col.value.name;
                let val = row.get(attr_name).or_else(|| row.get(attr_name)).cloned();
                if let Some(v) = val {
                    m.insert(attr_name.clone(), v);
                }
            }

            // Initialize OneToMany / ManyToMany arrays as empty
            for nav in &model.navigation_properties {
                if matches!(nav.kind, NavigationPropertyKind::OneToMany { .. })
                    || matches!(nav.kind, NavigationPropertyKind::ManyToMany)
                {
                    m.insert(nav.var_name.clone(), serde_json::Value::Array(vec![]));
                }
            }

            serde_json::Value::Object(m)
        });

        // Given some include tree, we can traverse navigation properties, adding only those that
        // appear in the tree.
        let Some(tree) = include_tree.as_ref() else {
            continue;
        };

        if let Value::Object(model_json) = model_json {
            process_navigation_properties(model_json, model, "", tree, row, ast)?;
        }
    }

    Ok(result_map.into_values().collect())
}

fn process_navigation_properties(
    model_json: &mut Map<String, Value>,
    model: &Model,
    prefix: &str,
    include_tree: &IncludeTreeJson,
    row: &Map<String, Value>,
    ast: &CloesceAst,
) -> Result<()> {
    for nav_prop in &model.navigation_properties {
        // Skip any property not in the tree.
        if !include_tree.contains_key(&nav_prop.var_name) {
            continue;
        }

        let nested_model = match ast.models.get(&nav_prop.model_reference) {
            Some(m) => m,
            None => fail!(OrmErrorKind::UnknownModel, "{}", nav_prop.model_reference),
        };

        // Nested properties always use their navigation path prefix (e.g. "cat.toy.id")
        // Check all primary key columns for the nested model
        let mut nested_pk_values = Vec::new();
        let mut all_nested_pks_present = true;

        for pk_col in &nested_model.primary_key_columns {
            let nested_pk_name = &pk_col.value.name;
            let prefixed_key = if prefix.is_empty() {
                format!("{}.{}", nav_prop.var_name, nested_pk_name)
            } else {
                format!("{}.{}.{}", prefix, nav_prop.var_name, nested_pk_name)
            };

            if let Some(nested_pk_value) = row.get(&prefixed_key) {
                if nested_pk_value.is_null() {
                    all_nested_pks_present = false;
                    break;
                }
                nested_pk_values.push((nested_pk_name.clone(), nested_pk_value.clone()));
            } else {
                all_nested_pks_present = false;
                break;
            }
        }

        if !all_nested_pks_present {
            continue;
        }

        // Build nested JSON object
        let mut nested_model_json = serde_json::Map::new();
        for (nested_pk_name, nested_pk_value) in &nested_pk_values {
            nested_model_json.insert(nested_pk_name.clone(), nested_pk_value.clone());
        }

        // Set nested scalar columns
        for col in &nested_model.columns {
            let attr_name = &col.value.name;
            let val = row
                .get(&format!("{}.{}.{}", prefix, nav_prop.var_name, attr_name))
                .or_else(|| row.get(&format!("{}.{}", nav_prop.var_name, attr_name)))
                .cloned();
            if let Some(v) = val {
                nested_model_json.insert(attr_name.clone(), v);
            }
        }

        // Initialize navigation property arrays
        for nested_nav_prop in &nested_model.navigation_properties {
            if matches!(
                nested_nav_prop.kind,
                NavigationPropertyKind::OneToMany { .. }
            ) || matches!(nested_nav_prop.kind, NavigationPropertyKind::ManyToMany)
            {
                nested_model_json.insert(nested_nav_prop.var_name.clone(), Value::Array(vec![]));
            }
        }

        // Recursively process the nested model if it's in the include tree
        if let Some(Value::Object(nested_include_tree)) = include_tree.get(&nav_prop.var_name) {
            let prefix = if prefix.is_empty() {
                nav_prop.var_name.clone()
            } else {
                format!("{prefix}.{}", nav_prop.var_name)
            };
            process_navigation_properties(
                &mut nested_model_json,
                nested_model,
                prefix.as_str(),
                nested_include_tree,
                row,
                ast,
            )?;
        }

        if matches!(nav_prop.kind, NavigationPropertyKind::OneToMany { .. })
            || matches!(nav_prop.kind, NavigationPropertyKind::ManyToMany)
        {
            if let Value::Array(arr) = model_json.get_mut(&nav_prop.var_name).unwrap() {
                // Check if this nested object already exists by comparing all primary key values
                let already_exists = arr.iter().any(|existing| {
                    nested_pk_values
                        .iter()
                        .all(|(pk_name, pk_value)| existing.get(pk_name) == Some(pk_value))
                });

                if !already_exists {
                    arr.push(Value::Object(nested_model_json));
                }
            }
        } else {
            model_json.insert(nav_prop.var_name.clone(), Value::Object(nested_model_json));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use ast::{CidlType, ForeignKeyReference, NavigationPropertyKind};
    use base64::{Engine, prelude::BASE64_STANDARD};
    use generator_test::{ModelBuilder, create_ast};
    use serde_json::{Map, Value, json};
    use sqlx::{Column, Row, SqlitePool, TypeInfo, sqlite::SqliteRow};

    use crate::methods::{map::map_sql, test_sql, upsert::UpsertModel};

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

                        "BLOB" => row
                            .try_get::<Vec<u8>, _>(name.as_str())
                            .map(|b| Value::String(BASE64_STANDARD.encode(&b)))
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
        // Arrange
        let horse = ModelBuilder::new("Horse")
            .id_pk()
            .col("name", CidlType::nullable(CidlType::Text), None, None)
            .nav_p(
                "riders",
                "Rider",
                NavigationPropertyKind::OneToMany {
                    key_columns: vec!["id".into()],
                },
            )
            .build();

        let rider = ModelBuilder::new("Rider")
            .id_pk()
            .col("nickname", CidlType::nullable(CidlType::Text), None, None)
            .build();

        let ast = create_ast(vec![horse, rider]);

        let rows: Vec<Map<String, Value>> = vec![];

        // Act
        let result = map_sql("Horse", rows, None, &ast).unwrap();

        // Assert
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn flat() {
        // Arrange
        let horse = ModelBuilder::new("Horse")
            .id_pk()
            .col("name", CidlType::nullable(CidlType::Text), None, None)
            .build();

        let ast = create_ast(vec![horse]);

        let row = vec![
            ("id".to_string(), json!("1")),
            ("name".to_string(), json!("Lightning")),
        ]
        .into_iter()
        .collect::<Map<String, Value>>();

        // Act
        let result = map_sql("Horse", vec![row], None, &ast).unwrap();
        let horse = result.first().unwrap().as_object().unwrap();

        // Assert
        assert_eq!(horse.get("id"), Some(&json!("1")));
        assert_eq!(horse.get("name"), Some(&json!("Lightning")));
    }

    #[sqlx::test]
    async fn one_to_one(db: SqlitePool) {
        // Arrange
        let ast = || {
            // hack: function to avoid lifetime issues
            create_ast(vec![
                ModelBuilder::new("Horse")
                    .id_pk()
                    .col("name", CidlType::nullable(CidlType::Text), None, None)
                    .col(
                        "best_rider_id",
                        CidlType::Integer,
                        Some(ForeignKeyReference {
                            model_name: "Rider".into(),
                            column_name: "id".into(),
                        }),
                        None,
                    )
                    .nav_p(
                        "best_rider",
                        "Rider",
                        NavigationPropertyKind::OneToOne {
                            key_columns: vec!["best_rider_id".into()],
                        },
                    )
                    .build(),
                ModelBuilder::new("Rider")
                    .id_pk()
                    .col("nickname", CidlType::nullable(CidlType::Text), None, None)
                    .build(),
            ])
        };

        let new_model = json!({
            "id": 1,
            "name": "Shadowfax",
            "best_rider_id": 1,
            "best_rider": {
                "id": 1,
                "nickname": "Gandalf"
            }
        });

        let include_tree = json!({
            "best_rider": {}
        });

        let upsert_res = UpsertModel::query(
            "Horse",
            &ast(),
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
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

        // Act
        let result = map_sql(
            "Horse",
            select_rows,
            Some(include_tree.as_object().unwrap().clone()),
            &ast(),
        )
        .expect("map_sql to succeed");

        assert_eq!(result, vec![new_model]);
    }

    #[sqlx::test]
    async fn one_to_many(db: SqlitePool) {
        // Arrange
        let ast = || {
            create_ast(vec![
                ModelBuilder::new("Horse")
                    .id_pk()
                    .col("name", CidlType::nullable(CidlType::Text), None, None)
                    .nav_p(
                        "riders",
                        "Rider",
                        NavigationPropertyKind::OneToMany {
                            key_columns: vec!["horse_id".into()],
                        },
                    )
                    .build(),
                ModelBuilder::new("Rider")
                    .id_pk()
                    .col("nickname", CidlType::nullable(CidlType::Text), None, None)
                    .col(
                        "horse_id",
                        CidlType::Integer,
                        Some(ForeignKeyReference {
                            model_name: "Horse".into(),
                            column_name: "id".into(),
                        }),
                        None,
                    )
                    .build(),
            ])
        };

        let new_model = json!({
            "id": 1,
            "name": "Black Beauty",
            "riders": [
                {
                    "id": 1,
                    "nickname": "Alice",
                    "horse_id": 1
                },
                {
                    "id": 2,
                    "nickname": "Bob",
                    "horse_id": 1
                }
            ]
        });

        let include_tree = json!({
            "riders": {}
        });

        let upsert_stmts = UpsertModel::query(
            "Horse",
            &ast(),
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
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

        // Act
        let result = map_sql(
            "Horse",
            select_rows,
            Some(include_tree.as_object().unwrap().clone()),
            &ast(),
        )
        .expect("map_sql to succeed");

        assert_eq!(result, vec![new_model]);
    }

    #[sqlx::test]
    async fn many_to_many(db: SqlitePool) {
        // Arrange
        let meta = || {
            create_ast(vec![
                ModelBuilder::new("Student")
                    .id_pk()
                    .col("name", CidlType::nullable(CidlType::Text), None, None)
                    .nav_p("courses", "Course", NavigationPropertyKind::ManyToMany)
                    .build(),
                ModelBuilder::new("Course")
                    .id_pk()
                    .col("title", CidlType::nullable(CidlType::Text), None, None)
                    .nav_p("students", "Student", NavigationPropertyKind::ManyToMany)
                    .build(),
            ])
        };

        let new_model = json!({
            "id": 1,
            "name": "John Doe",
            "courses": [
                {
                    "id": 1,
                    "title": "Math 101",
                    "students": []
                },
                {
                    "id": 2,
                    "title": "History 201",
                    "students": []
                }
            ]
        });

        let include_tree = json!({
            "courses": {}
        });

        let upsert_stmts = UpsertModel::query(
            "Student",
            &meta(),
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
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

        // Act
        let result = map_sql(
            "Student",
            select_rows,
            Some(include_tree.as_object().unwrap().clone()),
            &meta(),
        )
        .expect("map_sql to succeed");

        assert_eq!(result, vec![new_model]);
    }

    #[sqlx::test]
    async fn composite_primary_key(db: SqlitePool) {
        // Arrange
        let meta = || {
            create_ast(vec![
                ModelBuilder::new("Order")
                    .id_pk()
                    .col("order_date", CidlType::nullable(CidlType::Text), None, None)
                    .nav_p(
                        "items",
                        "OrderItem",
                        NavigationPropertyKind::OneToMany {
                            key_columns: vec!["order_id".into()],
                        },
                    )
                    .build(),
                ModelBuilder::new("Product")
                    .id_pk()
                    .col("name", CidlType::nullable(CidlType::Text), None, None)
                    .col("price", CidlType::Integer, None, None)
                    .build(),
                ModelBuilder::new("OrderItem")
                    .foreign_pk(
                        "order_id",
                        CidlType::Integer,
                        ForeignKeyReference {
                            model_name: "Order".into(),
                            column_name: "id".into(),
                        },
                    )
                    .foreign_pk(
                        "product_id",
                        CidlType::Integer,
                        ForeignKeyReference {
                            model_name: "Product".into(),
                            column_name: "id".into(),
                        },
                    )
                    .col("quantity", CidlType::Integer, None, None)
                    .nav_p(
                        "order",
                        "Order",
                        NavigationPropertyKind::OneToOne {
                            key_columns: vec!["order_id".into()],
                        },
                    )
                    .nav_p(
                        "product",
                        "Product",
                        NavigationPropertyKind::OneToOne {
                            key_columns: vec!["product_id".into()],
                        },
                    )
                    .build(),
            ])
        };

        let new_model = json!({
            "id": 1,
            "order_date": "2026-03-06",
            "items": [
                {
                    "order_id": 1,
                    "product_id": 101,
                    "quantity": 2,
                    "product": {
                        "id": 101,
                        "name": "Widget",
                        "price": 500
                    }
                },
                {
                    "order_id": 1,
                    "product_id": 102,
                    "quantity": 1,
                    "product": {
                        "id": 102,
                        "name": "Gadget",
                        "price": 750
                    }
                }
            ]
        });

        let include_tree_upsert = json!({
            "items": {
                "product": {}
            }
        });

        let include_tree_map = json!({
            "items": {}
        });

        let upsert_stmts = UpsertModel::query(
            "Order",
            &meta(),
            new_model.as_object().unwrap().clone(),
            Some(include_tree_upsert.as_object().unwrap().clone()),
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

        // Act
        let result = map_sql(
            "Order",
            select_rows,
            Some(include_tree_map.as_object().unwrap().clone()),
            &meta(),
        )
        .expect("map_sql to succeed");

        // Assert - Should correctly map order with items that have composite PKs
        assert_eq!(result.len(), 1);
        let order = result.first().unwrap().as_object().unwrap();
        assert_eq!(order.get("id"), Some(&json!(1)));
        assert_eq!(order.get("order_date"), Some(&json!("2026-03-06")));

        let items = order.get("items").unwrap().as_array().unwrap();
        assert_eq!(items.len(), 2);

        // Check first item has both composite PK values
        let item1 = items[0].as_object().unwrap();
        assert_eq!(item1.get("order_id"), Some(&json!(1)));
        assert_eq!(item1.get("product_id"), Some(&json!(101)));
        assert_eq!(item1.get("quantity"), Some(&json!(2)));

        // Check second item
        let item2 = items[1].as_object().unwrap();
        assert_eq!(item2.get("order_id"), Some(&json!(1)));
        assert_eq!(item2.get("product_id"), Some(&json!(102)));
        assert_eq!(item2.get("quantity"), Some(&json!(1)));
    }

    #[test]
    fn composite_primary_key_deduplication() {
        // Test that models with composite PKs are correctly deduplicated
        let ast = create_ast(vec![
            ModelBuilder::new("OrderItem")
                .pk("order_id", CidlType::Integer)
                .pk("product_id", CidlType::Integer)
                .col("quantity", CidlType::Integer, None, None)
                .build(),
        ]);

        // Two rows with the same composite PK should result in one model
        let rows = vec![
            vec![
                ("order_id".to_string(), json!(1)),
                ("product_id".to_string(), json!(101)),
                ("quantity".to_string(), json!(2)),
            ]
            .into_iter()
            .collect::<Map<String, Value>>(),
            vec![
                ("order_id".to_string(), json!(1)),
                ("product_id".to_string(), json!(101)),
                ("quantity".to_string(), json!(2)),
            ]
            .into_iter()
            .collect::<Map<String, Value>>(),
        ];

        // Act
        let result = map_sql("OrderItem", rows, None, &ast).unwrap();

        // Assert - Should only have one item despite two rows
        assert_eq!(result.len(), 1);
        let item = result.first().unwrap().as_object().unwrap();
        assert_eq!(item.get("order_id"), Some(&json!(1)));
        assert_eq!(item.get("product_id"), Some(&json!(101)));
        assert_eq!(item.get("quantity"), Some(&json!(2)));
    }
}
