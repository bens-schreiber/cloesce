use std::collections::{HashMap, HashSet};

use common::NavigationPropertyKind::{ManyToMany, OneToMany};
use common::{CidlType, Model, NamedTypedValue, NavigationPropertyKind};
use sea_query::{
    Alias, CommonTableExpression, SelectStatement, SimpleExpr, SqliteQueryBuilder,
    SubQueryStatement, WithQuery,
};
use sea_query::{InsertStatement, Query};
use serde_json::Map;
use serde_json::Value;

use crate::IncludeTree;
use crate::ModelMeta;
use crate::methods::{alias, push_scalar_value};

pub struct InsertModel<'a> {
    context: HashSet<String>,
    with_query: WithQuery,
    meta: &'a ModelMeta,
}

impl<'a> InsertModel<'a> {
    /// Given a model, traverses topological order accumulating insert statements.
    ///
    /// Allows for empty primary keys and foreign keys, inferring the value is auto generated
    /// or context driven through navigation properties.
    ///
    /// Returns a string of SQL statements, or a descriptive error string.
    pub fn query(
        model_name: &str,
        meta: &'a ModelMeta,
        new_model: Map<String, Value>,
        include_tree: Option<&IncludeTree>,
    ) -> Result<String, String> {
        let mut generator = Self {
            context: HashSet::default(),
            with_query: WithQuery::new(),
            meta,
        };

        generator.accumulate(
            None,
            model_name,
            &new_model,
            include_tree,
            model_name.to_string(),
        )?;

        let model = meta.get(model_name).unwrap();

        Ok(generator
            .with_query
            .query(
                SelectStatement::new()
                    .column(alias(format!("{}_{}", model_name, model.primary_key.name)))
                    .from(alias(model_name))
                    .to_owned(),
            )
            .to_string(SqliteQueryBuilder))
    }

    /// Recursive entrypoint for [InsertModel]
    fn accumulate(
        &mut self,
        parent_model_name: Option<&String>,
        model_name: &str,
        new_model: &Map<String, Value>,
        include_tree: Option<&IncludeTree>,
        path: String,
    ) -> Result<(), String> {
        let model = match self.meta.get(model_name) {
            Some(m) => m,
            None => return Err(format!("Unknown model {model_name}")),
        };

        let mut builder = InsertBuilder::new(model_name);

        // Primary key
        let pk = new_model.get(&model.primary_key.name);
        match pk {
            Some(val) => {
                builder.push_val(&model.primary_key.name, val, &model.primary_key.cidl_type)?;
            }
            None if matches!(model.primary_key.cidl_type, CidlType::Integer) => {
                // Generated
            }
            _ => {
                // Only integer primary keys can be left blank and generated.
                return Err(format!(
                    "Missing primary key for {}: {}",
                    model.name,
                    serde_json::to_string(new_model).unwrap()
                ));
            }
        };

        // Add all scalars, attempt to add FK's from value or context
        let mut fks_to_resolve = HashMap::<&str, &NamedTypedValue>::new();
        {
            // Cheating a little here. If this model is depends on another, it's dependency will have been inserted
            // before this model. Thus, it's parent pk has been inserted into the context under this path, which
            // follows the form Model.attr or Model.attr.<N>
            let parent_id_path = parent_model_name.map(|parent_model_name| {
                let base = path.split('_').next().unwrap_or(&path);
                format!(
                    "{}_{}",
                    base,
                    self.meta.get(parent_model_name).unwrap().primary_key.name
                )
            });

            for attr in &model.attributes {
                match (new_model.get(&attr.value.name), &attr.foreign_key_reference) {
                    (Some(value), _) => {
                        // A value was provided in `new_model`
                        builder.push_val(&attr.value.name, value, &attr.value.cidl_type)?;
                    }
                    (None, Some(fk_model)) if Some(fk_model) == parent_model_name => {
                        // A value was not provided, but the context contained one
                        let path_key = parent_id_path.as_ref().unwrap();
                        builder.use_var(&attr.value.name, path_key, &model.primary_key.name)?;
                    }
                    (None, None) => {
                        // A value was not provided and cannot be inferred.
                        return Err(format!(
                            "Missing attribute {} on {}: {}",
                            attr.value.name,
                            model.name,
                            serde_json::to_string(&new_model).unwrap()
                        ));
                    }
                    _ => {
                        // Delay resolving the FK until we've explored all navigation properties
                        // and expanded the context
                        fks_to_resolve.insert(&attr.value.name, &attr.value);
                    }
                };
            }
        };

        // Navigation properties, using the include tree as a traversal guide
        if let Some(include_tree) = include_tree {
            let (one_to_ones, others): (Vec<_>, Vec<_>) = model
                .navigation_properties
                .iter()
                .partition(|n| matches!(n.kind, NavigationPropertyKind::OneToOne { .. }));

            // This table is dependent on it's 1:1 references, so they must be traversed before
            // table insertion
            for nav in one_to_ones {
                let Some(Value::Object(nav_model)) = new_model.get(&nav.var_name) else {
                    continue;
                };
                let Some(Value::Object(nested_tree)) = include_tree.get(&nav.var_name) else {
                    continue;
                };
                let NavigationPropertyKind::OneToOne { reference } = &nav.kind else {
                    continue;
                };

                // Recursively handle nested inserts
                self.accumulate(
                    Some(&model.name),
                    &nav.model_name,
                    nav_model,
                    Some(nested_tree),
                    format!("{path}_{}", nav.var_name),
                )?;

                // Resolve deferred foreign key values with the updated context
                let nav_pk = &self.meta.get(&nav.model_name).unwrap().primary_key;
                let path_key = format!("{path}_{}_{}", nav.var_name, nav_pk.name);
                if let Some(ntv) = fks_to_resolve.remove(reference.as_str())
                    && self.context.contains(&path_key)
                {
                    builder.use_var(&ntv.name, &path_key, &nav_pk.name)?;
                }
            }

            // 1:M and M:M should be inserted _after_ this table
            self.insert_table(&path, model, &fks_to_resolve, new_model, builder)?;

            for nav in others {
                let Some(Value::Object(nested_tree)) = include_tree.get(&nav.var_name) else {
                    continue;
                };

                match (&nav.kind, new_model.get(&nav.var_name)) {
                    (OneToMany { .. }, Some(Value::Array(nav_models))) => {
                        for (i, nav_model) in
                            nav_models.iter().filter_map(|v| v.as_object()).enumerate()
                        {
                            self.accumulate(
                                Some(&model.name),
                                &nav.model_name,
                                nav_model,
                                Some(nested_tree),
                                format!("{path}_{}_{i}", nav.var_name),
                            )?;
                        }
                    }
                    (ManyToMany { unique_id }, Some(Value::Array(nav_models))) => {
                        for (i, nav_model) in
                            nav_models.iter().filter_map(|v| v.as_object()).enumerate()
                        {
                            self.accumulate(
                                Some(&model.name),
                                &nav.model_name,
                                nav_model,
                                Some(nested_tree),
                                format!("{path}_{}_{i}", nav.var_name),
                            )?;

                            let mut vars_to_use: Vec<(String, String, &String)> = vec![];

                            // Find the M:M dependencies id from context
                            {
                                let nav_pk = &self.meta.get(&nav.model_name).unwrap().primary_key;
                                let path_key =
                                    format!("{path}_{}_{i}_{}", nav.var_name, nav_pk.name);
                                self.context.get(&path_key).ok_or(format!(
                                    "Expected many to many model to contain an ID, got {}\n{:?}",
                                    serde_json::to_string(new_model).unwrap(),
                                    self.context.iter().collect::<Vec<_>>()
                                ))?;

                                vars_to_use.push((
                                    format!("{}_{}", nav.model_name, nav_pk.name),
                                    path_key,
                                    &nav_pk.name,
                                ));
                            }

                            // Get this models ID
                            vars_to_use.push((
                                format!("{}_{}", model.name, model.primary_key.name),
                                format!("{path}_{}", model.primary_key.name),
                                &model.primary_key.name,
                            ));

                            // Add the jct tables cols+vals
                            let mut jct_builder = InsertBuilder::new(unique_id);
                            vars_to_use.sort_by_key(|(name, _, _)| name.clone());
                            for (var_name, path, pk_name) in vars_to_use {
                                jct_builder.use_var(&var_name, &path, pk_name)?;
                            }

                            // TODO: We could do some kind of union all setup instead of
                            // CTE's for each M:M
                            self.with_query.cte(
                                CommonTableExpression::new()
                                    .query::<InsertStatement>(jct_builder.build(None))
                                    .table_name(alias(format!("{unique_id}_{i}")))
                                    .to_owned(),
                            );
                        }
                    }
                    _ => {
                        // Ignore
                    }
                }
            }

            return Ok(());
        }

        self.insert_table(&path, model, &fks_to_resolve, new_model, builder)?;
        Ok(())
    }

    /// Inserts the current [InsertBuilder] table, updating the graph context to include
    /// the tables id.
    ///
    /// Returns an error if foreign key values exist that can not be resolved.
    fn insert_table(
        &mut self,
        path: &str,
        model: &Model,
        fks_to_resolve: &HashMap<&str, &NamedTypedValue>,
        new_model: &Map<String, Value>,
        builder: InsertBuilder,
    ) -> Result<(), String> {
        let id_path = format!("{path}_{}", model.primary_key.name);
        self.with_query.cte(
            CommonTableExpression::new()
                .query::<InsertStatement>(builder.build(Some(&model.primary_key.name)))
                .table_name(alias(&id_path))
                .to_owned(),
        );
        self.context.insert(id_path);

        if !fks_to_resolve.is_empty() {
            return Err(format!(
                "Missing foreign key definitions for {}, got: {}",
                model.name,
                serde_json::to_string(new_model).unwrap()
            ));
        }

        Ok(())
    }
}

struct InsertBuilder {
    model_name: String,
    insert: InsertStatement,
    cols: Vec<Alias>,
    vals: Vec<SimpleExpr>,
}

impl InsertBuilder {
    fn new(model_name: &str) -> InsertBuilder {
        let mut insert = InsertStatement::new();
        insert.into_table(alias(model_name));
        Self {
            insert,
            model_name: model_name.to_string(),
            cols: Vec::default(),
            vals: Vec::default(),
        }
    }

    fn push_val(
        &mut self,
        var_name: &str,
        value: &Value,
        cidl_type: &CidlType,
    ) -> Result<(), String> {
        self.cols.push(alias(var_name));
        push_scalar_value(value, cidl_type, &self.model_name, var_name, &mut self.vals)
    }

    fn use_var(&mut self, var_name: &str, path: &str, pk_name: &String) -> Result<(), String> {
        self.cols.push(alias(var_name));
        self.vals.push(SimpleExpr::SubQuery(
            None,
            Box::new(SubQueryStatement::SelectStatement(
                Query::select()
                    .column(alias(pk_name))
                    .from(alias(path))
                    .to_owned(),
            )),
        ));

        Ok(())
    }

    fn build(mut self, pk_name: Option<&String>) -> InsertStatement {
        self.insert.columns(self.cols);
        self.insert.values_panic(self.vals);
        self.insert.or_default_values();
        if let Some(pk_name) = pk_name {
            self.insert.returning_col(alias(pk_name));
        } else {
            self.insert.returning_all();
        }

        self.insert
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use common::{CidlType, NavigationPropertyKind, builder::ModelBuilder};
    use serde_json::json;

    use crate::{expected_str, methods::insert::InsertModel};

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
        let res = InsertModel::query(
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
        let res =
            InsertModel::query("Note", &meta, model.as_object().unwrap().clone(), None).unwrap();

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
        let res = InsertModel::query(
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
        let res = InsertModel::query(
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
        let res = InsertModel::query(
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
            "WITH \"Person_id\" AS (INSERT INTO \"Person\" (\"id\") VALUES (1) RETURNING \"id\") , \"Person_horses_0_id\" AS (INSERT INTO \"Horse\" (\"id\", \"personId\") VALUES (1, 1) RETURNING \"id\") , \"Person_horses_1_id\" AS (INSERT INTO \"Horse\" (\"id\", \"personId\") VALUES (2, 1) RETURNING \"id\") , \"Person_horses_2_id\" AS (INSERT INTO \"Horse\" (\"id\", \"personId\") VALUES (3, 1) RETURNING \"id\") SELECT \"Person_id\" FROM \"Person\""
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
        let res = InsertModel::query(
            "Person",
            &model_meta,
            new_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        expected_str!(
            res,
            "WITH \"Person_id\" AS (INSERT INTO \"Person\" (\"id\") VALUES (1) RETURNING \"id\") , \"Person_horses_0_id\" AS (INSERT INTO \"Horse\" (\"id\") VALUES (1) RETURNING \"id\") , \"PersonsHorses_0\" AS (INSERT INTO \"PersonsHorses\" (\"Horse_id\", \"Person_id\") VALUES ((SELECT \"id\" FROM \"Person_horses_0_id\"), (SELECT \"id\" FROM \"Person_id\")) RETURNING *) , \"Person_horses_1_id\" AS (INSERT INTO \"Horse\" (\"id\") VALUES (2) RETURNING \"id\") , \"PersonsHorses_1\" AS (INSERT INTO \"PersonsHorses\" (\"Horse_id\", \"Person_id\") VALUES ((SELECT \"id\" FROM \"Person_horses_1_id\"), (SELECT \"id\" FROM \"Person_id\")) RETURNING *) SELECT \"Person_id\" FROM \"Person\""
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
        let res = InsertModel::query(
            "Person",
            &meta,
            new_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        expected_str!(
            res,
            r#"WITH "Person_horse_id" AS (INSERT INTO "Horse" ("id", "personId") VALUES (10, 1) RETURNING "id") , "Person_horse_awards_0_id" AS (INSERT INTO "Award" ("id", "horseId", "title") VALUES (100, 10, 'Fastest Horse') RETURNING "id") , "Person_horse_awards_1_id" AS (INSERT INTO "Award" ("id", "horseId", "title") VALUES (101, 10, 'Strongest Horse') RETURNING "id") , "Person_id" AS (INSERT INTO "Person" ("id", "horseId") VALUES (1, 10) RETURNING "id") SELECT "#
        );
    }

    #[test]
    fn missing_pk_autogenerates() {
        // Arrange
        let person = ModelBuilder::new("Person").id().build();
        let mut meta = std::collections::HashMap::new();
        meta.insert(person.name.clone(), person);

        let new_person = json!({});

        // Act
        let res = InsertModel::query(
            "Person",
            &meta,
            new_person.as_object().unwrap().clone(),
            None,
        )
        .unwrap();

        // Assert
        expected_str!(
            res,
            r#"WITH "Person_id" AS (INSERT INTO "Person" DEFAULT VALUES RETURNING "id") SELECT "Person_id" FROM "Person""#
        )
    }

    #[test]
    fn missing_one_to_one_fk_autogenerates() {
        let person = ModelBuilder::new("Person")
            .id()
            .attribute("horseId", CidlType::Integer, Some("Horse".into()))
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    reference: "horseId".into(),
                },
            )
            .build();

        let horse = ModelBuilder::new("Horse").id().build();

        let mut meta = std::collections::HashMap::new();
        meta.insert(person.name.clone(), person);
        meta.insert(horse.name.clone(), horse);

        let new_person = json!({
            "horse": {
                // Note that `new_person` has no pk, and that `horse` has no pk
            }
        });

        let include_tree = json!({
            "horse": {}
        });

        // Act
        let res = InsertModel::query(
            "Person",
            &meta,
            new_person.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap()),
        )
        .unwrap();

        // Assert
        expected_str!(
            res,
            r#"WITH "Person_horse_id" AS (INSERT INTO "Horse" DEFAULT VALUES RETURNING "id") , "Person_id" AS (INSERT INTO "Person" ("horseId") VALUES ((SELECT "id" FROM "Person_horse_id")) RETURNING "id") SELECT "Person_id" FROM "Person""#
        );
    }

    #[test]
    fn missing_one_to_many_fk_autogenerates() {
        // Arrange
        let person = ModelBuilder::new("Person")
            .id()
            .nav_p(
                "horses",
                "Horse",
                NavigationPropertyKind::OneToMany {
                    reference: "personId".into(),
                },
            )
            .build();

        let horse = ModelBuilder::new("Horse")
            .id()
            .attribute("personId", CidlType::Integer, Some("Person".into()))
            .build();

        let mut meta = std::collections::HashMap::new();
        meta.insert(person.name.clone(), person);
        meta.insert(horse.name.clone(), horse);

        let new_person = json!({
            "horses": [
                {
                    // totally empty horse
                    // should be able to infer it's personId
                }
            ]
        });

        let include_tree = json!({
            "horses": {}
        });

        // Act
        let res = InsertModel::query(
            "Person",
            &meta,
            new_person.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap()),
        )
        .unwrap();

        // Assert
        expected_str!(
            res,
            "WITH \"Person_id\" AS (INSERT INTO \"Person\" DEFAULT VALUES RETURNING \"id\") , \"Person_horses_0_id\" AS (INSERT INTO \"Horse\" (\"personId\") VALUES ((SELECT \"id\" FROM \"Person_id\")) RETURNING \"id\") SELECT \"Person_id\" FROM \"Person\""
        );
    }

    #[test]
    fn missing_many_to_many_pk_fk_autogenerates() {
        // Arrange
        let person = ModelBuilder::new("Person")
            .id()
            .nav_p(
                "horses",
                "Horse",
                NavigationPropertyKind::ManyToMany {
                    unique_id: "PersonsHorses".to_string(),
                },
            )
            .build();

        let horse = ModelBuilder::new("Horse").id().build();

        let mut meta = std::collections::HashMap::new();
        meta.insert(person.name.clone(), person);
        meta.insert(horse.name.clone(), horse);

        // new_person has no pk, horses array has no pks
        let new_person = json!({
            "horses": [
                {}, // empty horse
                {}  // another empty horse
            ]
        });

        let include_tree = json!({
            "horses": {}
        });

        // Act
        let res = InsertModel::query(
            "Person",
            &meta,
            new_person.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap()),
        )
        .unwrap();

        // Assert
        expected_str!(
            res,
            "WITH \"Person_id\" AS (INSERT INTO \"Person\" DEFAULT VALUES RETURNING \"id\") , \"Person_horses_0_id\" AS (INSERT INTO \"Horse\" DEFAULT VALUES RETURNING \"id\") , \"PersonsHorses_0\" AS (INSERT INTO \"PersonsHorses\" (\"Horse_id\", \"Person_id\") VALUES ((SELECT \"id\" FROM \"Person_horses_0_id\"), (SELECT \"id\" FROM \"Person_id\")) RETURNING *) , \"Person_horses_1_id\" AS (INSERT INTO \"Horse\" DEFAULT VALUES RETURNING \"id\") , \"PersonsHorses_1\" AS (INSERT INTO \"PersonsHorses\" (\"Horse_id\", \"Person_id\") VALUES ((SELECT \"id\" FROM \"Person_horses_1_id\"), (SELECT \"id\" FROM \"Person_id\")) RETURNING *) SELECT \"Person_id\" FROM \"Person\""
        );
    }
}
