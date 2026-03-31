use ast::CloesceAst;
use ast::Model;
use ast::NavigationFieldKind;
use indexmap::IndexMap;
use serde_json::Map;
use serde_json::Value;

use crate::OrmErrorKind;
use crate::fail;

use super::Result;

type D1Result = Vec<Map<String, Value>>;
type IncludeTreeJson = Map<String, Value>;

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
        for col in &model.primary_columns {
            let name = &col.field.name;
            if let Some(value) = row.get(name) {
                pk_values.push((name.clone(), value.clone()));
            } else {
                all_pks_present = false;
                break;
            }
        }

        if !all_pks_present {
            // Fail silently by skipping the row.
            continue;
        }

        // Create a composite key for the result map
        let composite_key = Value::Array(pk_values.iter().map(|(_, v)| v.clone()).collect());

        // A particular primary key will only exist once. If that key does not yet
        // exist, put a new model into the result map.
        let model_json = result_map.entry(composite_key).or_insert_with(|| {
            let mut m = serde_json::Map::new();

            // Set primary key columns
            for (name, value) in &pk_values {
                m.insert(name.clone(), value.clone());
            }

            // Set scalar columns
            for col in &model.columns {
                let name = &col.field.name;
                let val = row.get(name).or_else(|| row.get(name)).cloned();
                if let Some(v) = val {
                    m.insert(name.clone(), v);
                }
            }

            // Initialize OneToMany / ManyToMany arrays as empty
            for nav in &model.navigation_fields {
                if matches!(nav.kind, NavigationFieldKind::OneToMany { .. })
                    || matches!(nav.kind, NavigationFieldKind::ManyToMany)
                {
                    m.insert(nav.field.name.clone(), serde_json::Value::Array(vec![]));
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
    for nav_prop in &model.navigation_fields {
        // Skip any property not in the tree.
        if !include_tree.contains_key(&nav_prop.field.name) {
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

        for pk in &nested_model.primary_columns {
            let nested_pk_name = &pk.field.name;
            let prefixed_key = if prefix.is_empty() {
                format!("{}.{}", nav_prop.field.name, nested_pk_name)
            } else {
                format!("{}.{}.{}", prefix, nav_prop.field.name, nested_pk_name)
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
        for (name, value) in &nested_pk_values {
            nested_model_json.insert(name.clone(), value.clone());
        }

        // Set nested scalar columns
        for col in &nested_model.columns {
            let name = &col.field.name;
            let val = row
                .get(&format!("{}.{}.{}", prefix, nav_prop.field.name, name))
                .or_else(|| row.get(&format!("{}.{}", nav_prop.field.name, name)))
                .cloned();
            if let Some(v) = val {
                nested_model_json.insert(name.clone(), v);
            }
        }

        // Initialize navigation property arrays
        for nested_nav_prop in &nested_model.navigation_fields {
            if matches!(nested_nav_prop.kind, NavigationFieldKind::OneToMany { .. })
                || matches!(nested_nav_prop.kind, NavigationFieldKind::ManyToMany)
            {
                nested_model_json.insert(nested_nav_prop.field.name.clone(), Value::Array(vec![]));
            }
        }

        // Recursively process the nested model if it's in the include tree
        if let Some(Value::Object(nested_include_tree)) = include_tree.get(&nav_prop.field.name) {
            let prefix = if prefix.is_empty() {
                nav_prop.field.name.clone()
            } else {
                format!("{prefix}.{}", nav_prop.field.name)
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

        if matches!(nav_prop.kind, NavigationFieldKind::OneToMany { .. })
            || matches!(nav_prop.kind, NavigationFieldKind::ManyToMany)
        {
            if let Value::Array(arr) = model_json.get_mut(&nav_prop.field.name).unwrap() {
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
            model_json.insert(
                nav_prop.field.name.clone(),
                Value::Object(nested_model_json),
            );
        }
    }

    Ok(())
}
