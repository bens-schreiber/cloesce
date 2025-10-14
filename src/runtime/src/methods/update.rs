use common::NavigationPropertyKind::{ManyToMany, OneToMany, OneToOne};
use sea_query::SqliteQueryBuilder;
use sea_query::UpdateStatement;
use serde_json::Map;
use serde_json::Value;

use crate::IncludeTree;
use crate::ModelMeta;
use crate::methods::{alias, push_scalar_value};

pub fn update_model(
    model_name: &str,
    meta: &ModelMeta,
    updated_model: Map<String, Value>,
    include_tree: Option<&IncludeTree>,
) -> Result<String, String> {
    let mut acc = vec![];
    topo_ordered_updates(model_name, meta, &updated_model, include_tree, &mut acc)?;

    Ok(acc
        .iter()
        .map(|i| format!("{};", i.to_string(SqliteQueryBuilder)))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn topo_ordered_updates(
    model_name: &str,
    meta: &ModelMeta,
    updated_model: &Map<String, Value>,
    include_tree: Option<&IncludeTree>,
    acc: &mut Vec<UpdateStatement>,
) -> Result<(), String> {
    let model = match meta.get(model_name) {
        Some(m) => m,
        None => return Err(format!("Unknown model {model_name}")),
    };

    let mut update = UpdateStatement::new();
    update.table(alias(&model.name));

    let mut columns = vec![];
    let mut scalar_values = vec![];

    // Check updated primary key
    if let Some(pk) = updated_model.get(&model.primary_key.name) {
        columns.push(alias(&model.primary_key.name));
        push_scalar_value(
            pk,
            &model.primary_key.cidl_type,
            model_name,
            &model.primary_key.name,
            &mut scalar_values,
        )?;
    }

    // Check updated attributes
    for attr in &model.attributes {
        let Some(value) = updated_model.get(&attr.value.name) else {
            continue;
        };

        columns.push(alias(&attr.value.name));
        push_scalar_value(
            value,
            &attr.value.cidl_type,
            model_name,
            &attr.value.name,
            &mut scalar_values,
        )?;
    }

    update.values(columns.into_iter().zip(scalar_values));

    // Check updated nav props
    if let Some(include_tree) = include_tree {
        for nav in &model.navigation_properties {
            let Some(Value::Object(nested_tree)) = include_tree.get(&nav.var_name) else {
                continue;
            };

            match (&nav.kind, updated_model.get(&nav.var_name)) {
                (OneToOne { .. }, Some(Value::Object(nav_model))) => {
                    topo_ordered_updates(&nav.model_name, meta, nav_model, Some(nested_tree), acc)?;
                }
                (OneToMany { .. } | ManyToMany { .. }, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models.iter().filter_map(|v| v.as_object()) {
                        topo_ordered_updates(
                            &nav.model_name,
                            meta,
                            nav_model,
                            Some(nested_tree),
                            acc,
                        )?;
                    }
                }
                _ => {
                    continue;
                }
            }
        }
    }

    acc.push(update);

    Ok(())
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use common::{CidlType, NavigationPropertyKind, builder::ModelBuilder};
    use serde_json::json;

    use crate::{expected_str, methods::update::update_model};

    #[test]
    fn scalar_models() {
        // Arrange
        let ast_model = ModelBuilder::new("Horse")
            .id()
            .attribute("color", CidlType::Text, None)
            .attribute("age", CidlType::Integer, None)
            .attribute("address", CidlType::nullable(CidlType::Text), None)
            .build();

        let updated_model = json!({
            "id": 1,
            "color": "black",
            "age": 8,
            "address": "barn"
        });

        let mut model_meta = HashMap::new();
        model_meta.insert(ast_model.name.clone(), ast_model);

        // Act
        let res = update_model(
            "Horse",
            &model_meta,
            updated_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap();

        // Assert
        expected_str!(
            res,
            r#"UPDATE "Horse" SET "id" = 1, "color" = 'black', "age" = 8, "address" = 'barn'"#
        );
    }

    #[test]
    fn nullable_text_null_ok() {
        // Arrange
        let ast_model = ModelBuilder::new("Note")
            .id()
            .attribute("content", CidlType::nullable(CidlType::Text), None)
            .build();

        let mut meta = HashMap::new();
        meta.insert(ast_model.name.clone(), ast_model);

        let updated_model = json!({
            "id": 5,
            "content": null
        });

        // Act
        let res = update_model(
            "Note",
            &meta,
            updated_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap();

        // Assert
        expected_str!(res, r#"UPDATE "Note" SET "id" = 5, "content" = null"#);
    }

    #[test]
    fn one_to_one() {
        // Arrange
        let ast_person = ModelBuilder::new("Person")
            .id()
            .attribute("horseId", CidlType::Integer, Some("Horse".into()))
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    reference: "horseId".to_string(),
                },
            )
            .build();

        let ast_horse = ModelBuilder::new("Horse")
            .id()
            .attribute("age", CidlType::Integer, None)
            .build();

        let updated_model = json!({
            "id": 1,
            "horseId": 1,
            "horse": {
                "id": 1,
                "age": 10
            }
        });

        let include_tree = json!({
            "horse": {}
        });

        let mut model_meta = HashMap::new();
        model_meta.insert(ast_horse.name.clone(), ast_horse);
        model_meta.insert(ast_person.name.clone(), ast_person);

        // Act
        let res = update_model(
            "Person",
            &model_meta,
            updated_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        expected_str!(res, r#"UPDATE "Horse" SET "id" = 1, "age" = 10"#);
        expected_str!(res, r#"UPDATE "Person" SET "id" = 1, "horseId" = 1"#);
    }

    #[test]
    fn one_to_many() {
        // Arrange
        let ast_person = ModelBuilder::new("Person")
            .id()
            .nav_p(
                "horses",
                "Horse",
                NavigationPropertyKind::OneToMany {
                    reference: "personId".to_string(),
                },
            )
            .build();
        let ast_horse = ModelBuilder::new("Horse")
            .id()
            .attribute("personId", CidlType::Integer, Some("Person".into()))
            .attribute("color", CidlType::Text, None)
            .build();

        let updated_model = json!({
            "id": 1,
            "horses": [
                {
                    "id": 1,
                    "color": "white"
                },
                {
                    "id": 2,
                    "color": "gray"
                }
            ]
        });

        let include_tree = json!({
            "horses": {}
        });

        let mut model_meta = HashMap::new();
        model_meta.insert(ast_horse.name.clone(), ast_horse);
        model_meta.insert(ast_person.name.clone(), ast_person);

        // Act
        let res = update_model(
            "Person",
            &model_meta,
            updated_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        expected_str!(res, r#"UPDATE "Horse" SET "id" = 1, "color" = 'white'"#);
        expected_str!(res, r#"UPDATE "Horse" SET "id" = 2, "color" = 'gray'"#);
        expected_str!(res, r#"UPDATE "Person" SET "id" = 1"#);
    }

    #[test]
    fn nav_props_no_include_tree() {
        // Arrange
        let ast_person = ModelBuilder::new("Person")
            .id()
            .attribute("horseId", CidlType::Integer, Some("Horse".into()))
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    reference: "horseId".to_string(),
                },
            )
            .build();
        let ast_horse = ModelBuilder::new("Horse").id().build();

        let updated_model = json!({
            "id": 1,
            "horseId": 1,
            "horse": {
                "id": 1,
            }
        });

        let mut model_meta = HashMap::new();
        model_meta.insert(ast_horse.name.clone(), ast_horse);
        model_meta.insert(ast_person.name.clone(), ast_person);

        // Act
        let res = update_model(
            "Person",
            &model_meta,
            updated_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap();

        // Assert
        expected_str!(res, r#"UPDATE "Person" SET "id" = 1, "horseId" = 1"#);
    }
}
