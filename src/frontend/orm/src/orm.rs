use std::collections::HashMap;

use ast::Model;
use ast::NavigationPropertyKind;
use serde_json::Map;
use serde_json::Value;

use crate::D1Result;
use crate::IncludeTree;
use crate::ModelMeta;

pub fn object_relational_mapping(
    model_name: &str,
    meta: &ModelMeta,
    rows: &D1Result,
    include_tree: &Option<IncludeTree>,
) -> Result<Vec<Value>, String> {
    let model = match meta.get(model_name) {
        Some(m) => m,
        None => return Err(format!("Unknown model {model_name}")),
    };

    let pk_name = &model.primary_key.name;
    let mut result_map: HashMap<Value, Value> = HashMap::new();

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

            // Set scalar attributes
            for attr in &model.attributes {
                let attr_name = &attr.value.name;
                let val = row.get(attr_name).or_else(|| row.get(attr_name)).cloned();
                if let Some(v) = val {
                    m.insert(attr_name.clone(), v);
                }
            }

            // Initialize OneToMany / ManyToMany arrays as empty
            for nav in &model.navigation_properties {
                if matches!(nav.kind, NavigationPropertyKind::OneToMany { .. })
                    || matches!(nav.kind, NavigationPropertyKind::ManyToMany { .. })
                {
                    m.insert(nav.var_name.clone(), serde_json::Value::Array(vec![]));
                }
            }

            serde_json::Value::Object(m)
        });

        // Given some include tree, we can traverse navigation properties, adding only those that
        // appear in the tree.
        if let Some(tree) = include_tree
            && let Value::Object(model_json) = model_json
        {
            process_navigation_properties(model_json, model, "", tree, row, meta)?;
        }
    }

    Ok(result_map.into_values().collect())
}

fn process_navigation_properties(
    model_json: &mut Map<String, Value>,
    model: &Model,
    prefix: &str,
    include_tree: &Map<String, Value>,
    row: &Map<String, Value>,
    meta: &ModelMeta,
) -> Result<(), String> {
    for nav_prop in &model.navigation_properties {
        // Skip any property not in the tree.
        if !include_tree.contains_key(&nav_prop.var_name) {
            continue;
        }

        let nested_model = match meta.get(&nav_prop.model_name) {
            Some(m) => m,
            None => return Err(format!("Unknown model {}.", nav_prop.model_name)),
        };

        // Nested properties always use their navigation path prefix (e.g. "cat.toy.id")
        let nested_pk_name = &nested_model.primary_key.name;
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

        // Set nested scalar attributes
        for attr in &nested_model.attributes {
            let attr_name = &attr.value.name;
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
            ) || matches!(
                nested_nav_prop.kind,
                NavigationPropertyKind::ManyToMany { .. }
            ) {
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
            || matches!(nav_prop.kind, NavigationPropertyKind::ManyToMany { .. })
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
    use ast::{CidlType, NavigationPropertyKind, builder::ModelBuilder};
    use serde_json::{Map, Value, json};
    use std::collections::HashMap;

    use crate::orm::object_relational_mapping;

    #[test]
    fn no_records_returns_empty() {
        // Arrange
        let horse = ModelBuilder::new("Horse")
            .id()
            .attribute("name", CidlType::nullable(CidlType::Text), None)
            .nav_p(
                "riders",
                "Rider",
                NavigationPropertyKind::OneToMany {
                    reference: "id".into(),
                },
            )
            .build();

        let rider = ModelBuilder::new("Rider")
            .id()
            .attribute("nickname", CidlType::nullable(CidlType::Text), None)
            .build();

        let meta = HashMap::from([("Horse".to_string(), horse), ("Rider".to_string(), rider)]);

        let rows: Vec<Map<String, Value>> = vec![];
        let include_tree: Option<Map<String, Value>> = None;

        // Act
        let result = object_relational_mapping("Horse", &meta, &rows, &include_tree).unwrap();

        // Assert
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn flat() {
        // Arrange
        let horse = ModelBuilder::new("Horse")
            .id()
            .attribute("name", CidlType::nullable(CidlType::Text), None)
            .build();

        let meta = HashMap::from([("Horse".to_string(), horse)]);

        let row = vec![
            ("id".to_string(), json!("1")),
            ("name".to_string(), json!("Lightning")),
        ]
        .into_iter()
        .collect::<Map<String, Value>>();

        let include_tree: Option<Map<String, Value>> = None;

        // Act
        let result = object_relational_mapping("Horse", &meta, &vec![row], &include_tree).unwrap();
        let horse = result.first().unwrap().as_object().unwrap();

        // Assert
        assert_eq!(horse.get("id"), Some(&json!("1")));
        assert_eq!(horse.get("name"), Some(&json!("Lightning")));
    }

    #[test]
    fn assigns_scalar_attributes_and_navigation_arrays() {
        // Arrange
        let horse = ModelBuilder::new("Horse")
            .id()
            .attribute("name", CidlType::nullable(CidlType::Text), None)
            .nav_p(
                "riders",
                "Rider",
                NavigationPropertyKind::OneToMany {
                    reference: "id".into(),
                },
            )
            .build();

        let rider = ModelBuilder::new("Rider")
            .id()
            .attribute("nickname", CidlType::nullable(CidlType::Text), None)
            .build();

        let meta = HashMap::from([("Horse".to_string(), horse), ("Rider".to_string(), rider)]);

        // rows vector
        let rows: Vec<Map<String, Value>> = vec![
            vec![
                ("id".to_string(), json!("1")),
                ("name".to_string(), json!("Thunder")),
                ("riders.id".to_string(), json!("r1")),
                ("riders.nickname".to_string(), json!("Speedy")),
            ]
            .into_iter()
            .collect(),
            vec![
                ("id".to_string(), json!("1")),
                ("name".to_string(), json!("Thunder")),
                ("riders.id".to_string(), json!("r2")),
                ("riders.nickname".to_string(), json!("Flash")),
            ]
            .into_iter()
            .collect(),
        ];

        let include_tree: Option<Map<String, Value>> = Some(
            vec![("riders".to_string(), json!({}))]
                .into_iter()
                .collect(),
        );

        // Act
        let result = object_relational_mapping("Horse", &meta, &rows, &include_tree).unwrap();
        let horse = result.first().unwrap().as_object().unwrap();

        // Assert
        assert_eq!(horse.get("id"), Some(&json!("1")));
        assert_eq!(horse.get("name"), Some(&json!("Thunder")));

        let riders = horse.get("riders").unwrap().as_array().unwrap();
        let ids: Vec<&Value> = riders.iter().map(|r| &r["id"]).collect();

        assert!(ids.contains(&&json!("r1")));
        assert!(ids.contains(&&json!("r2")));
    }

    #[test]
    fn merges_duplicate_rows_with_arrays() {
        // Arrange
        let horse = ModelBuilder::new("Horse")
            .id()
            .attribute("name", CidlType::nullable(CidlType::Text), None)
            .nav_p(
                "riders",
                "Rider",
                NavigationPropertyKind::OneToMany {
                    reference: "id".into(),
                },
            )
            .build();

        let rider = ModelBuilder::new("Rider")
            .id()
            .attribute("nickname", CidlType::nullable(CidlType::Text), None)
            .build();

        let meta = HashMap::from([("Horse".to_string(), horse), ("Rider".to_string(), rider)]);

        let rows: Vec<Map<String, Value>> = vec![
            vec![
                ("id".to_string(), json!("1")),
                ("name".to_string(), json!("hoarse")),
                ("riders.id".to_string(), json!("r1")),
                ("riders.nickname".to_string(), json!("Speedy")),
            ]
            .into_iter()
            .collect(),
            vec![
                ("id".to_string(), json!("1")),
                ("name".to_string(), json!("hoarse")),
                ("riders.id".to_string(), json!("r1")),
                ("riders.nickname".to_string(), json!("Speedy")),
            ]
            .into_iter()
            .collect(),
            vec![
                ("id".to_string(), json!("1")),
                ("name".to_string(), json!("hoarse")),
                ("riders.id".to_string(), json!("r2")),
                ("riders.nickname".to_string(), json!("Flash")),
            ]
            .into_iter()
            .collect(),
        ];

        let include_tree = Some(
            vec![("riders".to_string(), json!({}))]
                .into_iter()
                .collect::<Map<String, Value>>(),
        );

        // Act
        let result = object_relational_mapping("Horse", &meta, &rows, &include_tree).unwrap();
        let horse = result.first().unwrap().as_object().unwrap();
        let riders = horse.get("riders").unwrap().as_array().unwrap();

        // Assert
        assert_eq!(riders.len(), 2);

        let ids: Vec<&Value> = riders.iter().map(|r| &r["id"]).collect();
        assert!(ids.contains(&&json!("r1")));
        assert!(ids.contains(&&json!("r2")));
    }
}
