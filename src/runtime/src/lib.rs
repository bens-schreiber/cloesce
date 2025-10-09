use common::Model;
use common::NavigationPropertyKind;
use serde_json::Map;
use serde_json::Value;

use std::collections::HashMap;
use std::slice;
use std::str;

type D1Result = Vec<Map<String, serde_json::Value>>;
type ModelMeta = HashMap<String, Model>;
type IncludeTree = Option<Map<String, serde_json::Value>>;

static mut RETURN_LEN: usize = 0;

#[unsafe(no_mangle)]
pub extern "C" fn get_return_len() -> usize {
    unsafe { RETURN_LEN }
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

#[unsafe(no_mangle)]
pub extern "C" fn dealloc(ptr: *mut u8, cap: usize) {
    unsafe {
        let _ = Vec::from_raw_parts(ptr, 0, cap);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn object_relational_mapping(
    // Model Name
    model_name_ptr: *const u8,
    model_name_len: usize,

    // Meta AST
    meta_ptr: *const u8,
    meta_len: usize,

    // SQL result rows
    rows_ptr: *const u8,
    rows_len: usize,

    // Include Tree
    include_tree_ptr: *const u8,
    include_tree_len: usize,
) -> *const u8 {
    let model_name =
        unsafe { str::from_utf8(slice::from_raw_parts(model_name_ptr, model_name_len)).unwrap() };
    let meta_json = unsafe { str::from_utf8(slice::from_raw_parts(meta_ptr, meta_len)).unwrap() };
    let rows_json = unsafe { str::from_utf8(slice::from_raw_parts(rows_ptr, rows_len)).unwrap() };
    let include_tree_json = unsafe {
        str::from_utf8(slice::from_raw_parts(include_tree_ptr, include_tree_len)).unwrap()
    };

    let meta = serde_json::from_str::<ModelMeta>(&meta_json).unwrap();
    let rows = serde_json::from_str::<D1Result>(&rows_json).unwrap();
    let include_tree = serde_json::from_str::<IncludeTree>(&include_tree_json).unwrap();

    let res = _object_relational_mapping(model_name, &meta, &rows, &include_tree);
    let json_str = serde_json::to_string(&res).unwrap();

    let mut bytes = json_str.into_bytes();
    let ptr = bytes.as_mut_ptr();
    unsafe {
        RETURN_LEN = bytes.len();
        std::mem::forget(bytes); // leak so JS can read it
    }
    ptr
}

fn _object_relational_mapping(
    model_name: &str,
    meta: &ModelMeta,
    rows: &D1Result,
    include_tree: &IncludeTree,
) -> Vec<Value> {
    let model = match meta.get(model_name) {
        Some(m) => m,
        None => return vec![],
    };

    let pk_name = &model.primary_key.name;
    let mut result_map: HashMap<Value, Value> = HashMap::new();

    // Scan each row for the root model (`model_name`)'s primary key
    for row in rows.into_iter() {
        let Some(pk_value) = row
            .get(&format!("{}.{}", model_name, pk_name))
            .or_else(|| row.get(pk_name))
        else {
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
                let val = row
                    .get(&format!("{}.{}", model_name, attr_name))
                    .or_else(|| row.get(attr_name))
                    .cloned();
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
            process_navigation_properties(model_json, model, model_name, tree, row, meta);
        }
    }

    result_map.into_values().collect()
}

fn process_navigation_properties(
    model_json: &mut Map<String, Value>,
    model: &Model,
    prefix: &str,
    include_tree: &Map<String, Value>,
    row: &Map<String, Value>,
    meta: &ModelMeta,
) {
    for nav_prop in &model.navigation_properties {
        // Skip any property not in the tree.
        if !include_tree.contains_key(&nav_prop.var_name) {
            continue;
        }

        let nested_model = match meta.get(&nav_prop.model_name) {
            Some(m) => m,
            None => continue,
        };

        // NOTE: No need to check for non prefixed keys here, we can assume a nav prop
        // comes from a view and thus will be prefixed
        let nested_pk_name = &nested_model.primary_key.name;
        let prefixed_key = format!("{}.{}.{}", prefix, nav_prop.var_name, nested_pk_name);
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
            let prefix = format!("{prefix}.{}", nav_prop.var_name);
            process_navigation_properties(
                &mut nested_model_json,
                nested_model,
                prefix.as_str(),
                nested_include_tree,
                row,
                meta,
            );
        }

        if matches!(nav_prop.kind, NavigationPropertyKind::OneToMany { .. })
            || matches!(nav_prop.kind, NavigationPropertyKind::ManyToMany { .. })
        {
            if let Value::Array(arr) = model_json.get_mut(&nav_prop.var_name).unwrap() {
                let already_exists = arr.iter().any(|existing| {
                    existing
                        .get(nested_pk_name)
                        .map_or(false, |v| v == nested_pk_value)
                });

                if !already_exists {
                    arr.push(Value::Object(nested_model_json));
                }
            }
        } else {
            model_json.insert(nav_prop.var_name.clone(), Value::Object(nested_model_json));
        }
    }
}

#[cfg(test)]
mod tests {
    use common::{CidlType, NavigationPropertyKind, builder::ModelBuilder};
    use serde_json::{Map, Value, json};
    use std::collections::HashMap;

    use crate::_object_relational_mapping;

    #[test]
    fn returns_empty_array_if_no_records() {
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

        let result = _object_relational_mapping("Horse", &meta, &rows, &include_tree);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn handles_non_prefixed_columns_correctly() {
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

        let result = _object_relational_mapping("Horse", &meta, &vec![row], &include_tree);
        let horse = result.first().unwrap().as_object().unwrap();
        assert_eq!(horse.get("id"), Some(&json!("1")));
        assert_eq!(horse.get("name"), Some(&json!("Lightning")));
    }

    #[test]
    fn handles_prefixed_columns_correctly() {
        let horse = ModelBuilder::new("Horse")
            .id()
            .attribute("name", CidlType::nullable(CidlType::Text), None)
            .build();

        let meta = HashMap::from([("Horse".to_string(), horse)]);

        let row = vec![
            ("Horse.id".to_string(), json!("1")),
            ("Horse.name".to_string(), json!("Thunder")),
        ]
        .into_iter()
        .collect::<Map<String, Value>>();

        let include_tree: Option<Map<String, Value>> = None;

        let result = _object_relational_mapping("Horse", &meta, &vec![row], &include_tree);
        let horse = result.first().unwrap().as_object().unwrap();
        assert_eq!(horse.get("id"), Some(&json!("1")));
        assert_eq!(horse.get("name"), Some(&json!("Thunder")));
    }

    #[test]
    fn assigns_scalar_attributes_and_navigation_arrays_correctly() {
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
                ("Horse.id".to_string(), json!("1")),
                ("Horse.name".to_string(), json!("Thunder")),
                ("Horse.riders.id".to_string(), json!("r1")),
                ("Horse.riders.nickname".to_string(), json!("Speedy")),
            ]
            .into_iter()
            .collect(),
            vec![
                ("Horse.id".to_string(), json!("1")),
                ("Horse.name".to_string(), json!("Thunder")),
                ("Horse.riders.id".to_string(), json!("r2")),
                ("Horse.riders.nickname".to_string(), json!("Flash")),
            ]
            .into_iter()
            .collect(),
        ];

        let include_tree: Option<Map<String, Value>> = Some(
            vec![("riders".to_string(), json!({}))]
                .into_iter()
                .collect(),
        );

        let result = _object_relational_mapping("Horse", &meta, &rows, &include_tree);
        let horse = result.first().unwrap().as_object().unwrap();

        assert_eq!(horse.get("id"), Some(&json!("1")));
        assert_eq!(horse.get("name"), Some(&json!("Thunder")));

        let riders = horse.get("riders").unwrap().as_array().unwrap();
        let ids: Vec<&Value> = riders.iter().map(|r| &r["id"]).collect();

        assert!(ids.contains(&&json!("r1")));
        assert!(ids.contains(&&json!("r2")));
    }

    #[test]
    fn merges_duplicate_rows_with_arrays() {
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
                ("Horse.id".to_string(), json!("1")),
                ("Horse.name".to_string(), json!("hoarse")),
                ("Horse.riders.id".to_string(), json!("r1")),
                ("Horse.riders.nickname".to_string(), json!("Speedy")),
            ]
            .into_iter()
            .collect(),
            vec![
                ("Horse.id".to_string(), json!("1")),
                ("Horse.name".to_string(), json!("hoarse")),
                ("Horse.riders.id".to_string(), json!("r1")),
                ("Horse.riders.nickname".to_string(), json!("Speedy")),
            ]
            .into_iter()
            .collect(),
            vec![
                ("Horse.id".to_string(), json!("1")),
                ("Horse.name".to_string(), json!("hoarse")),
                ("Horse.riders.id".to_string(), json!("r2")),
                ("Horse.riders.nickname".to_string(), json!("Flash")),
            ]
            .into_iter()
            .collect(),
        ];

        let include_tree = Some(
            vec![("riders".to_string(), json!({}))]
                .into_iter()
                .collect::<Map<String, Value>>(),
        );

        let result = _object_relational_mapping("Horse", &meta, &rows, &include_tree);
        let horse = result.first().unwrap().as_object().unwrap();
        let riders = horse.get("riders").unwrap().as_array().unwrap();

        // Ensure no duplicates, just r1 and r2
        assert_eq!(riders.len(), 2);

        let ids: Vec<&Value> = riders.iter().map(|r| &r["id"]).collect();
        assert!(ids.contains(&&json!("r1")));
        assert!(ids.contains(&&json!("r2")));
    }
}
