use common::NavigationPropertyKind;
use common::NavigationPropertyKind::{ManyToMany, OneToMany};
use sea_query::InsertStatement;
use sea_query::SqliteQueryBuilder;
use serde_json::Map;
use serde_json::Value;

use crate::IncludeTree;
use crate::ModelMeta;
use crate::methods::{alias, push_scalar_value};

pub fn insert_model(
    model_name: &str,
    meta: &ModelMeta,
    new_model: Map<String, Value>,
    include_tree: Option<&IncludeTree>,
) -> Result<String, String> {
    let mut acc = vec![];
    topo_ordered_inserts(model_name, meta, &new_model, include_tree, &mut acc)?;

    Ok(acc
        .iter()
        .map(|i| format!("{};", i.to_string(SqliteQueryBuilder)))
        .collect::<Vec<_>>()
        .join("\n"))
}

/// Returns a list of table insertions in topological order (dependency comes before dependent)
fn topo_ordered_inserts(
    model_name: &str,
    meta: &ModelMeta,
    new_model: &Map<String, Value>,
    include_tree: Option<&IncludeTree>,
    acc: &mut Vec<InsertStatement>,
) -> Result<(), String> {
    let model = match meta.get(model_name) {
        Some(m) => m,
        None => return Err(format!("Unknown model {model_name}")),
    };

    let mut insert = InsertStatement::new();
    insert.into_table(alias(&model.name));

    // Primary key
    let pk = new_model.get(&model.primary_key.name).ok_or(format!(
        "Expected a primary key to exist on the model {model_name}"
    ))?;

    let mut scalar_cols = vec![alias(&model.primary_key.name)];
    let mut scalar_vals = vec![];
    push_scalar_value(
        pk,
        &model.primary_key.cidl_type,
        &model.name,
        &model.primary_key.name,
        &mut scalar_vals,
    )?;

    // Scalar properties
    // TODO: We can try to infer a foreign key's ID through some context stack.
    for attr in &model.attributes {
        let value = new_model.get(&attr.value.name).ok_or(&format!(
            "Attribute {} to exist on new model",
            attr.value.name
        ))?;

        push_scalar_value(
            value,
            &attr.value.cidl_type,
            &model.name,
            &attr.value.name,
            &mut scalar_vals,
        )?;

        scalar_cols.push(alias(&attr.value.name));
    }

    insert.columns(scalar_cols);
    insert.values_panic(scalar_vals);

    // Navigation properties, using the include tree as a guide
    if let Some(include_tree) = include_tree {
        let (one_to_ones, others): (Vec<_>, Vec<_>) = model
            .navigation_properties
            .iter()
            .partition(|n| matches!(n.kind, NavigationPropertyKind::OneToOne { .. }));

        // One to One table must be created before this table
        for nav in one_to_ones {
            let Some(Value::Object(nav_model)) = new_model.get(&nav.var_name) else {
                continue;
            };
            let Some(Value::Object(nested_tree)) = include_tree.get(&nav.var_name) else {
                continue;
            };

            topo_ordered_inserts(&nav.model_name, meta, nav_model, Some(nested_tree), acc)?;
        }

        acc.push(insert);

        let mut jcts = vec![];

        // One to Many tables must be created after this table
        for nav in others {
            let Some(Value::Object(nested_tree)) = include_tree.get(&nav.var_name) else {
                continue;
            };

            match (&nav.kind, new_model.get(&nav.var_name)) {
                (OneToMany { .. }, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models.iter().filter_map(|v| v.as_object()) {
                        topo_ordered_inserts(
                            &nav.model_name,
                            meta,
                            nav_model,
                            Some(nested_tree),
                            acc,
                        )?;
                    }
                }
                (ManyToMany { unique_id }, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models.iter().filter_map(|v| v.as_object()) {
                        topo_ordered_inserts(
                            &nav.model_name,
                            meta,
                            nav_model,
                            Some(nested_tree),
                            acc,
                        )?;

                        // Build a junction table entry
                        let mut insert_jct = InsertStatement::new();
                        let nav_pk = &meta
                            .get(&nav.model_name)
                            .ok_or(format!(
                                "Expected a primary key to exist on the model {}",
                                nav.model_name
                            ))?
                            .primary_key;

                        // Jct tables are always the name of the unique_id, with columns
                        // "Model1.pk_name" "Model2.pk_name"
                        insert_jct.into_table(alias(unique_id));
                        insert_jct.columns(vec![
                            alias(format!("{}.{}", model.name, model.primary_key.name)),
                            alias(format!("{}.{}", nav.model_name, nav_pk.name)),
                        ]);

                        {
                            let mut jct_values = vec![];
                            let nav_pk_value = nav_model.get(&nav_pk.name).ok_or(format!(
                                "Expected a primary key to exist on the model {model_name}"
                            ))?;

                            push_scalar_value(
                                pk,
                                &model.primary_key.cidl_type,
                                &model.name,
                                &model.primary_key.name,
                                &mut jct_values,
                            )?;
                            push_scalar_value(
                                nav_pk_value,
                                &nav_pk.cidl_type,
                                &nav.model_name,
                                &nav_pk.name,
                                &mut jct_values,
                            )?;
                            insert_jct.values_panic(jct_values);
                        }

                        jcts.push(insert_jct)
                    }
                }
                _ => {
                    // Ignore
                }
            }
        }

        acc.append(&mut jcts);
        return Ok(());
    }

    acc.push(insert);
    Ok(())
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use common::{CidlType, NavigationPropertyKind, builder::ModelBuilder};
    use serde_json::json;

    use crate::{expected_str, methods::insert::insert_model};

    #[test]
    fn scalar_models() {
        // Arrange
        let ast_model = ModelBuilder::new("Horse")
            .id()
            .attribute("color", CidlType::Text, None)
            .attribute("age", CidlType::Integer, None)
            .attribute("address", CidlType::nullable(CidlType::Text), None)
            .build();

        let new_model = json!({
            "id": 1,
            "color": "brown",
            "age": 7,
            "address": null
        });

        let mut model_meta = HashMap::new();
        model_meta.insert(ast_model.name.clone(), ast_model);

        // Act
        let res = insert_model(
            "Horse",
            &model_meta,
            new_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap();

        // Assert
        expected_str!(
            res,
            r#"INSERT INTO "Horse" ("id", "color", "age", "address") VALUES (1, 'brown', 7, null)"#
        )
    }

    #[test]
    fn nullable_text_null_ok() {
        // Arrange
        let ast_model = ModelBuilder::new("Note")
            .id()
            .attribute("content", CidlType::nullable(CidlType::Text), None)
            .build();

        let mut meta = std::collections::HashMap::new();
        meta.insert(ast_model.name.clone(), ast_model);

        let model = serde_json::json!({
            "id": 5,
            "content": null
        });

        // Act
        let res = insert_model("Note", &meta, model.as_object().unwrap().clone(), None).unwrap();

        // Assert
        expected_str!(
            res,
            r#"INSERT INTO "Note" ("id", "content") VALUES (5, null)"#
        );
    }

    #[test]
    fn nav_props_no_include_tree() {
        // Arrange
        let ast_person = ModelBuilder::new("Person")
            .id()
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    reference: "horseId".to_string(),
                },
            )
            .build();
        let ast_horse = ModelBuilder::new("Horse").id().build();

        let new_model = json!({
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
        let res = insert_model(
            "Person",
            &model_meta,
            new_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap();

        // Assert
        expected_str!(res, r#"INSERT INTO "Person" ("id") VALUES (1)"#);
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
        let ast_horse = ModelBuilder::new("Horse").id().build();

        let new_model = json!({
            "id": 1,
            "horseId": 1,
            "horse": {
                "id": 1,
            }
        });

        let include_tree = json!({
            "horse": {}
        });

        let mut model_meta = HashMap::new();
        model_meta.insert(ast_horse.name.clone(), ast_horse);
        model_meta.insert(ast_person.name.clone(), ast_person);

        // Act
        let res = insert_model(
            "Person",
            &model_meta,
            new_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        expected_str!(
            res,
            r#"INSERT INTO "Person" ("id", "horseId") VALUES (1, 1)"#
        );
        expected_str!(res, r#"INSERT INTO "Horse" ("id") VALUES (1)"#);
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
            .build();

        let new_model = json!({
            "id": 1,
            "horses": [
                {
                    "id": 1,
                    "personId": 1
                },
                {
                    "id": 2,
                    "personId": 1
                },
                {
                    "id": 3,
                    "personId": 1
                },
            ]
        });

        let include_tree = json!({
            "horses": {}
        });

        let mut model_meta = HashMap::new();
        model_meta.insert(ast_horse.name.clone(), ast_horse);
        model_meta.insert(ast_person.name.clone(), ast_person);

        // Act
        let res = insert_model(
            "Person",
            &model_meta,
            new_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        expected_str!(res, r#"INSERT INTO "Person" ("id") VALUES (1)"#);
        expected_str!(
            res,
            r#"INSERT INTO "Horse" ("id", "personId") VALUES (1, 1);
INSERT INTO "Horse" ("id", "personId") VALUES (2, 1);
INSERT INTO "Horse" ("id", "personId") VALUES (3, 1);"#
        );
    }

    #[test]
    fn many_to_many() {
        // Arrange
        let ast_person = ModelBuilder::new("Person")
            .id()
            .nav_p(
                "horses",
                "Horse",
                NavigationPropertyKind::ManyToMany {
                    unique_id: "PersonsHorses".to_string(),
                },
            )
            .build();
        let ast_horse = ModelBuilder::new("Horse").id().build();

        let new_model = json!({
            "id": 1,
            "horses": [
                {
                    "id": 1,
                    "personId": 1
                },
                {
                    "id": 2,
                    "personId": 1
                },
            ]
        });

        let include_tree = json!({
            "horses": {}
        });

        let mut model_meta = HashMap::new();
        model_meta.insert(ast_horse.name.clone(), ast_horse);
        model_meta.insert(ast_person.name.clone(), ast_person);

        // Act
        let res = insert_model(
            "Person",
            &model_meta,
            new_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        expected_str!(res, r#"INSERT INTO "Person" ("id") VALUES (1)"#);
        expected_str!(
            res,
            r#"INSERT INTO "PersonsHorses" ("Person.id", "Horse.id") VALUES (1, 1);
INSERT INTO "PersonsHorses" ("Person.id", "Horse.id") VALUES (1, 2);"#
        );
    }

    #[test]
    fn topological_ordering_is_correct() {
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
            .attribute("personId", CidlType::Integer, Some("Person".into()))
            .nav_p(
                "awards",
                "Award",
                NavigationPropertyKind::OneToMany {
                    reference: "horseId".to_string(),
                },
            )
            .build();

        let ast_award = ModelBuilder::new("Award")
            .id()
            .attribute("horseId", CidlType::Integer, Some("Horse".into()))
            .attribute("title", CidlType::Text, None)
            .build();

        let mut meta = std::collections::HashMap::new();
        meta.insert(ast_person.name.clone(), ast_person);
        meta.insert(ast_horse.name.clone(), ast_horse);
        meta.insert(ast_award.name.clone(), ast_award);

        let new_model = json!({
            "id": 1,
            "horseId": 10,
            "horse": {
                "id": 10,
                "personId": 1,
                "awards": [
                    { "id": 100, "horseId": 10, "title": "Fastest Horse" },
                    { "id": 101, "horseId": 10, "title": "Strongest Horse" }
                ]
            }
        });

        let include_tree = json!({
            "horse": {
                "awards": {}
            }
        });

        // Act
        let res = insert_model(
            "Person",
            &meta,
            new_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        let inserts: Vec<_> = res
            .lines()
            .filter(|line| line.starts_with("INSERT"))
            .collect();

        assert!(
            inserts[0].contains("INSERT INTO \"Horse\""),
            "Expected Horse inserted first, got {}",
            inserts[0]
        );
        assert!(
            inserts[1].contains("INSERT INTO \"Award\""),
            "Expected Award inserted third, got {}",
            inserts[1]
        );
        assert!(
            inserts[2].contains("INSERT INTO \"Award\""),
            "Expected another Award insert, got {}",
            inserts[2]
        );
        assert!(
            inserts[3].contains("INSERT INTO \"Person\""),
            "Expected Person inserted second, got {}",
            inserts[3]
        );
    }
}
