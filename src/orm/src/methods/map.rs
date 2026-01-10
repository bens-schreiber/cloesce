use ast::Model;
use ast::NavigationPropertyKind;
use ast::fail;
use indexmap::IndexMap;
use serde_json::Map;
use serde_json::Value;

use crate::D1Result;
use crate::IncludeTreeJson;
use crate::ModelMeta;
use crate::methods::OrmErrorKind;

use super::Result;

pub fn map_sql(
    model_name: &str,
    rows: D1Result,
    include_tree: Option<IncludeTreeJson>,
    meta: &ModelMeta,
) -> Result<Vec<Value>> {
    let model = match meta.get(model_name) {
        Some(m) => m,
        None => fail!(OrmErrorKind::UnknownModel, "{}", model_name),
    };
    let Some(pk) = model.primary_key.as_ref() else {
        fail!(
            OrmErrorKind::ModelMissingD1,
            "Model {} is not a D1 model",
            model_name
        )
    };

    let pk_name = &pk.name;
    let mut result_map = IndexMap::new();

    // Scan each row for the root model (`model_name`)'s primary key
    for row in rows.iter() {
        let Some(pk_value) = row.get(pk_name).or_else(|| row.get(pk_name)) else {
            // The root models primary key is not in the row, this row
            // is not mappable.
            continue;
        };

        // A particular primary key will only exist once. If that key does not yet
        // exist, put a new model into the result map.
        let model_json = result_map.entry(pk_value.clone()).or_insert_with(|| {
            let mut m = serde_json::Map::new();

            // Set primary key
            m.insert(pk_name.clone(), pk_value.clone());

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
            process_navigation_properties(model_json, model, "", tree, row, meta)?;
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
    meta: &ModelMeta,
) -> Result<()> {
    for nav_prop in &model.navigation_properties {
        // Skip any property not in the tree.
        if !include_tree.contains_key(&nav_prop.var_name) {
            continue;
        }

        let nested_model = match meta.get(&nav_prop.model_reference) {
            Some(m) => m,
            None => fail!(OrmErrorKind::UnknownModel, "{}", nav_prop.model_reference),
        };

        // Nested properties always use their navigation path prefix (e.g. "cat.toy.id")
        let nested_pk_name = &nested_model.primary_key.as_ref().unwrap().name;
        let prefixed_key = if prefix.is_empty() {
            format!("{}.{}", nav_prop.var_name, nested_pk_name)
        } else {
            format!("{}.{}.{}", prefix, nav_prop.var_name, nested_pk_name)
        };
        let Some(nested_pk_value) = row.get(&prefixed_key) else {
            continue;
        };
        if nested_pk_value.is_null() {
            continue;
        }

        // Build nested JSON object
        let mut nested_model_json = serde_json::Map::new();
        nested_model_json.insert(nested_pk_name.clone(), nested_pk_value.clone());

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
                meta,
            )?;
        }

        if matches!(nav_prop.kind, NavigationPropertyKind::OneToMany { .. })
            || matches!(nav_prop.kind, NavigationPropertyKind::ManyToMany)
        {
            if let Value::Array(arr) = model_json.get_mut(&nav_prop.var_name).unwrap() {
                let already_exists = arr
                    .iter()
                    .any(|existing| existing.get(nested_pk_name) == Some(nested_pk_value));

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
    use ast::{CidlType, NavigationPropertyKind};
    use base64::{Engine, prelude::BASE64_STANDARD};
    use generator_test::ModelBuilder;
    use serde_json::{Map, Value, json};
    use sqlx::{Column, Row, SqlitePool, TypeInfo, sqlite::SqliteRow};
    use std::collections::HashMap;

    use crate::{
        ModelMeta,
        methods::{map::map_sql, test_sql, upsert::UpsertModel},
    };

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
            .col("name", CidlType::nullable(CidlType::Text), None)
            .nav_p(
                "riders",
                "Rider",
                NavigationPropertyKind::OneToMany {
                    column_reference: "id".into(),
                },
            )
            .build();

        let rider = ModelBuilder::new("Rider")
            .id_pk()
            .col("nickname", CidlType::nullable(CidlType::Text), None)
            .build();

        let meta = HashMap::from([("Horse".to_string(), horse), ("Rider".to_string(), rider)]);

        let rows: Vec<Map<String, Value>> = vec![];

        // Act
        let result = map_sql("Horse", rows, None, &meta).unwrap();

        // Assert
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn flat() {
        // Arrange
        let horse = ModelBuilder::new("Horse")
            .id_pk()
            .col("name", CidlType::nullable(CidlType::Text), None)
            .build();

        let meta = HashMap::from([("Horse".to_string(), horse)]);

        let row = vec![
            ("id".to_string(), json!("1")),
            ("name".to_string(), json!("Lightning")),
        ]
        .into_iter()
        .collect::<Map<String, Value>>();

        // Act
        let result = map_sql("Horse", vec![row], None, &meta).unwrap();
        let horse = result.first().unwrap().as_object().unwrap();

        // Assert
        assert_eq!(horse.get("id"), Some(&json!("1")));
        assert_eq!(horse.get("name"), Some(&json!("Lightning")));
    }

    #[sqlx::test]
    async fn one_to_one(db: SqlitePool) {
        // Arrange
        let meta = || {
            vec![
                ModelBuilder::new("Horse")
                    .id_pk()
                    .col("name", CidlType::nullable(CidlType::Text), None)
                    .col("best_rider_id", CidlType::Integer, Some("Rider".into()))
                    .nav_p(
                        "best_rider",
                        "Rider",
                        NavigationPropertyKind::OneToOne {
                            column_reference: "best_rider_id".into(),
                        },
                    )
                    .build(),
                ModelBuilder::new("Rider")
                    .id_pk()
                    .col("nickname", CidlType::nullable(CidlType::Text), None)
                    .build(),
            ]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect::<ModelMeta>()
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

        let upsert_stmts = UpsertModel::query(
            "Horse",
            &meta(),
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .expect("upsert to succeed");

        let results = test_sql(
            meta(),
            upsert_stmts
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
            &meta(),
        )
        .expect("map_sql to succeed");

        assert_eq!(result, vec![new_model]);
    }

    #[sqlx::test]
    async fn one_to_many(db: SqlitePool) {
        // Arrange
        let meta = || {
            vec![
                ModelBuilder::new("Horse")
                    .id_pk()
                    .col("name", CidlType::nullable(CidlType::Text), None)
                    .nav_p(
                        "riders",
                        "Rider",
                        NavigationPropertyKind::OneToMany {
                            column_reference: "horse_id".into(),
                        },
                    )
                    .build(),
                ModelBuilder::new("Rider")
                    .id_pk()
                    .col("nickname", CidlType::nullable(CidlType::Text), None)
                    .col("horse_id", CidlType::Integer, Some("Horse".into()))
                    .build(),
            ]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect::<ModelMeta>()
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
            &meta(),
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .expect("upsert to succeed");

        let results = test_sql(
            meta(),
            upsert_stmts
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
            &meta(),
        )
        .expect("map_sql to succeed");

        assert_eq!(result, vec![new_model]);
    }

    #[sqlx::test]
    async fn many_to_many(db: SqlitePool) {
        // Arrange
        let meta = || {
            vec![
                ModelBuilder::new("Student")
                    .id_pk()
                    .col("name", CidlType::nullable(CidlType::Text), None)
                    .nav_p("courses", "Course", NavigationPropertyKind::ManyToMany)
                    .build(),
                ModelBuilder::new("Course")
                    .id_pk()
                    .col("title", CidlType::nullable(CidlType::Text), None)
                    .nav_p("students", "Student", NavigationPropertyKind::ManyToMany)
                    .build(),
            ]
            .into_iter()
            .map(|m| (m.name.clone(), m))
            .collect::<ModelMeta>()
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

        for stmt in &upsert_stmts {
            println!("SQL: {}", stmt.query);
            println!("Values: {:?}", stmt.values);
        }

        let results = test_sql(
            meta(),
            upsert_stmts
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
}
