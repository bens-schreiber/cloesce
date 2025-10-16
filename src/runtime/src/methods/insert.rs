use std::collections::HashMap;

use common::NavigationPropertyKind::{ManyToMany, OneToMany};
use common::{CidlType, Model, NamedTypedValue, NavigationPropertyKind};
use sea_query::{Alias, SimpleExpr, SqliteQueryBuilder, SubQueryStatement, TableDropStatement};
use sea_query::{ColumnDef, Expr, InsertStatement, Query, TableCreateStatement};
use serde_json::Map;
use serde_json::Value;

use crate::IncludeTree;
use crate::ModelMeta;
use crate::methods::{alias, push_scalar_value};

#[derive(Default)]
pub struct InsertModel {
    context: HashMap<String, GraphContext>,
    acc: Vec<String>,
}

impl InsertModel {
    /// Given a model, traverses topological order accumulating insert statements.
    ///
    /// Allows for empty primary keys and foreign keys, inferring the value is auto generated
    /// or context driven through navigation properties.
    ///
    /// Returns a string of SQL statements, or a descriptive error string.
    pub fn query(
        model_name: &str,
        meta: &ModelMeta,
        new_model: Map<String, Value>,
        include_tree: Option<&IncludeTree>,
    ) -> Result<String, String> {
        // TODO: we don't always need a variables table
        let insert_var_table = VariablesTable::create().to_string(SqliteQueryBuilder);
        let drop_var_table = VariablesTable::drop().to_string(SqliteQueryBuilder);

        let mut generator = Self::default();
        generator.accumulate(
            meta,
            None,
            model_name,
            &new_model,
            include_tree,
            model_name.to_string(),
        )?;

        let topo_sorted_inserts = generator
            .acc
            .drain(..)
            .map(|stmt| format!("{stmt};"))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(format!(
            "{};\n{}\n{};",
            insert_var_table, topo_sorted_inserts, drop_var_table
        ))
    }

    /// Recursive entrypoint for [InsertModel]
    fn accumulate(
        &mut self,
        meta: &ModelMeta,
        parent_model_name: Option<&String>,
        model_name: &str,
        new_model: &Map<String, Value>,
        include_tree: Option<&IncludeTree>,
        path: String,
    ) -> Result<(), String> {
        let model = match meta.get(model_name) {
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
        let mut fks_to_resolve = self.resolve_scalars(
            meta,
            parent_model_name,
            model,
            &path,
            new_model,
            &mut builder,
        )?;

        // Navigation properties, using the include tree as a traversal guide
        if let Some(include_tree) = include_tree {
            self.recurse_navs_insert(
                pk,
                model,
                new_model,
                meta,
                include_tree,
                &mut fks_to_resolve,
                &path,
                builder,
            )?;
            return Ok(());
        }

        self.insert_table(pk, &path, model, &fks_to_resolve, &new_model, builder)?;
        Ok(())
    }

    /// Iterates all scalar properties, requiring non-fk's to be present.
    ///
    /// If a foreign key value is missing, tries to retrieve it from the context.
    ///
    /// Returns a map of foreign keys that cannot find a value.
    fn resolve_scalars<'a>(
        &mut self,
        meta: &ModelMeta,
        parent_model_name: Option<&String>,
        model: &'a Model,
        path: &str,
        new_model: &Map<String, Value>,
        builder: &mut InsertBuilder,
    ) -> Result<HashMap<&'a str, &'a NamedTypedValue>, String> {
        let mut fks_to_resolve = HashMap::<&str, &NamedTypedValue>::new();

        // Cheating here. If this model is depends on another, it's dependency will have been inserted
        // before this model. Thus, it's parent pk has been inserted into the context under this path.
        let parent_id_path = if let Some(parent_model_name) = parent_model_name {
            Some(format!(
                "{}.{}",
                path.rsplit_once('.').map(|(h, _)| h).unwrap_or(path),
                meta.get(parent_model_name).unwrap().primary_key.name
            ))
        } else {
            None
        };

        for attr in &model.attributes {
            match (new_model.get(&attr.value.name), &attr.foreign_key_reference) {
                (Some(value), _) => {
                    // A value was provided in `new_model`
                    builder.push_val(&attr.value.name, value, &attr.value.cidl_type)?;
                }
                (None, Some(fk_model)) if Some(fk_model) == parent_model_name.as_deref() => {
                    // A value was not provided, but the context contained one
                    let path_key = parent_id_path.as_ref().unwrap();

                    let ctx = self.context.get(path_key).unwrap();
                    builder.use_var(ctx, &attr.value.name, &attr.value.cidl_type, &path_key)?;
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

        Ok(fks_to_resolve)
    }

    /// Iterates through all navigation properties, first recursing through 1:1 values, then
    /// inserting the current table, then iterating through dependent 1:M / M:M values.
    fn recurse_navs_insert(
        &mut self,
        pk: Option<&Value>,
        model: &Model,
        new_model: &Map<String, Value>,
        meta: &ModelMeta,
        include_tree: &IncludeTree,
        fks_to_resolve: &mut HashMap<&str, &NamedTypedValue>,
        path: &str,
        mut builder: InsertBuilder,
    ) -> Result<(), String> {
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
                meta,
                Some(&model.name),
                &nav.model_name,
                nav_model,
                Some(nested_tree),
                format!("{path}.{}", nav.var_name),
            )?;

            // Resolve deferred foreign key values with the updated context
            let nav_pk = &meta.get(&nav.model_name).unwrap().primary_key;
            let path_key = format!("{path}.{}.{}", nav.var_name, nav_pk.name);
            if let Some(ntv) = fks_to_resolve.remove(reference.as_str())
                && let Some(ctx) = self.context.get(&path_key)
            {
                builder.use_var(ctx, &ntv.name, &nav_pk.cidl_type, &path_key)?;
            }
        }

        // 1:M and M:M should be inserted _after_ this table
        self.insert_table(pk, &path, model, &fks_to_resolve, &new_model, builder)?;

        for nav in others {
            let Some(Value::Object(nested_tree)) = include_tree.get(&nav.var_name) else {
                continue;
            };

            match (&nav.kind, new_model.get(&nav.var_name)) {
                (OneToMany { .. }, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models.iter().filter_map(|v| v.as_object()) {
                        self.accumulate(
                            meta,
                            Some(&model.name),
                            &nav.model_name,
                            nav_model,
                            Some(nested_tree),
                            format!("{path}.{}", nav.var_name),
                        )?;
                    }
                }
                (ManyToMany { unique_id }, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models.iter().filter_map(|v| v.as_object()) {
                        self.accumulate(
                            meta,
                            Some(&model.name),
                            &nav.model_name,
                            nav_model,
                            Some(nested_tree),
                            format!("{path}.{}", nav.var_name),
                        )?;

                        let mut vars_to_use: Vec<(String, &CidlType, &GraphContext, String)> =
                            vec![];

                        // Find the 1:M dependencies id from context
                        {
                            let nav_pk = &meta.get(&nav.model_name).unwrap().primary_key;
                            let path_key = format!("{path}.{}.{}", nav.var_name, nav_pk.name);
                            let ctx = self.context.get(&path_key).ok_or(format!(
                                "Expected many to many model to contain an ID, got {}",
                                serde_json::to_string(new_model).unwrap()
                            ))?;

                            vars_to_use.push((
                                format!("{}.{}", nav.model_name, nav_pk.name),
                                &nav_pk.cidl_type,
                                ctx,
                                path_key,
                            ));
                        }

                        // Get this models ID
                        let this_ctx = &match pk {
                            Some(val) => GraphContext::Value(val.clone()),
                            None => GraphContext::Variable,
                        };
                        vars_to_use.push((
                            format!("{}.{}", model.name, model.primary_key.name),
                            &model.primary_key.cidl_type,
                            this_ctx,
                            format!("{path}.{}", model.primary_key.name),
                        ));

                        // Add the jct tables cols+vals
                        let mut jct_builder = InsertBuilder::new(&unique_id);
                        vars_to_use.sort_by_key(|(name, _, _, _)| name.clone());
                        for (var_name, cidl_type, var_ctx, path) in vars_to_use {
                            jct_builder.use_var(&var_ctx, &var_name, cidl_type, &path)?;
                        }

                        self.acc.push(jct_builder.build());
                    }
                }
                _ => {
                    // Ignore
                }
            }
        }

        Ok(())
    }

    /// Inserts the current [InsertBuilder] table, updating the graph context to include
    /// the tables id.
    ///
    /// Returns an error if foreign key values exist that can not be resolved.
    fn insert_table(
        &mut self,
        pk: Option<&Value>,
        path: &str,
        model: &Model,
        fks_to_resolve: &HashMap<&str, &NamedTypedValue>,
        new_model: &Map<String, Value>,
        builder: InsertBuilder,
    ) -> Result<(), String> {
        self.acc.push(builder.build());

        // Add this tables primary key to the context so dependents can resolve it
        match pk {
            None => {
                let id_path = format!("{path}.{}", model.primary_key.name);
                self.acc.push(VariablesTable::insert_rowid(&id_path));
                self.context.insert(id_path, GraphContext::Variable);
            }
            Some(val) => {
                let id_path = format!("{path}.{}", model.primary_key.name);
                self.context
                    .insert(id_path, GraphContext::Value(val.clone()));
            }
        }

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

// A temporary table for resolving primary keys during insertion.
// TODO: We could also use CTE's. Just didn't feel like figuring out SeaQuery crap.
const VARIABLES_TABLE_NAME: &str = "tmp_paths";
const VARIABLES_TABLE_COL_PATH: &str = "path";
const VARIABLES_TABLE_COL_ID: &str = "id";
struct VariablesTable;
impl VariablesTable {
    fn create() -> TableCreateStatement {
        TableCreateStatement::new()
            .table(alias(VARIABLES_TABLE_NAME))
            .temporary()
            .col(
                ColumnDef::new(VARIABLES_TABLE_COL_PATH)
                    .primary_key()
                    .text(),
            )
            .col(ColumnDef::new(VARIABLES_TABLE_COL_ID).integer())
            .to_owned()
    }

    fn drop() -> TableDropStatement {
        TableDropStatement::new()
            .table(alias(VARIABLES_TABLE_NAME))
            .to_owned()
    }

    fn insert_rowid(path: &str) -> String {
        let mut insert = InsertStatement::new();
        insert.into_table(alias(VARIABLES_TABLE_NAME));
        insert.columns(vec![alias("path"), alias("id")]);
        insert.values_panic(vec![
            Expr::val(path).into(),
            Expr::cust("last_insert_rowid()"),
        ]);
        insert.replace();
        insert.to_string(SqliteQueryBuilder)
    }
}

#[derive(Debug)]
enum GraphContext {
    // Auto generated, stored in the variables table.
    Variable,

    // An actual supplied value from the `new_model`
    Value(Value),
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

    fn use_var(
        &mut self,
        ctx: &GraphContext,
        var_name: &str,
        cidl_type: &CidlType,
        path: &str,
    ) -> Result<(), String> {
        match ctx {
            GraphContext::Variable => {
                self.cols.push(alias(var_name));

                let subquery = SubQueryStatement::SelectStatement(
                    Query::select()
                        .from(alias(VARIABLES_TABLE_NAME))
                        .column(alias(VARIABLES_TABLE_COL_ID))
                        .and_where(Expr::col(alias(VARIABLES_TABLE_COL_PATH)).eq(path))
                        .to_owned(),
                );

                self.vals
                    .push(SimpleExpr::SubQuery(None, Box::new(subquery)));
            }
            GraphContext::Value(v) => {
                self.push_val(var_name, v, cidl_type)?;
            }
        }
        Ok(())
    }

    fn build(mut self) -> String {
        self.insert.columns(self.cols);
        self.insert.values_panic(self.vals);
        self.insert.or_default_values();
        self.insert.to_string(SqliteQueryBuilder)
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
            r#"INSERT INTO "Person" ("id") VALUES (1);
INSERT INTO "Horse" ("id") VALUES (1);
INSERT INTO "PersonsHorses" ("Horse.id", "Person.id") VALUES (1, 1);
INSERT INTO "Horse" ("id") VALUES (2);
INSERT INTO "PersonsHorses" ("Horse.id", "Person.id") VALUES (2, 1);
DROP TABLE "tmp_paths";"#
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
        let lines = res.split("\n").collect::<Vec<_>>();
        expected_str!(lines[0], "CREATE TEMPORARY TABLE");
        expected_str!(lines[1], "INSERT INTO \"Person\" DEFAULT VALUES;");
        expected_str!(
            lines[2],
            "REPLACE INTO \"tmp_paths\" (\"path\", \"id\") VALUES ('Person.id', last_insert_rowid());"
        );
        expected_str!(lines[3], "DROP TABLE \"tmp_paths\"")
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
            r#"
INSERT INTO "Horse" DEFAULT VALUES;
REPLACE INTO "tmp_paths" ("path", "id") VALUES ('Person.horse.id', last_insert_rowid());
INSERT INTO "Person" ("horseId") VALUES ((SELECT "id" FROM "tmp_paths" WHERE "path" = 'Person.horse.id'));
REPLACE INTO "tmp_paths" ("path", "id") VALUES ('Person.id', last_insert_rowid());"#
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
            r#"CREATE TEMPORARY TABLE "tmp_paths" ( "path" text PRIMARY KEY, "id" integer );
INSERT INTO "Person" DEFAULT VALUES;
REPLACE INTO "tmp_paths" ("path", "id") VALUES ('Person.id', last_insert_rowid());
INSERT INTO "Horse" ("personId") VALUES ((SELECT "id" FROM "tmp_paths" WHERE "path" = 'Person.id'));
REPLACE INTO "tmp_paths" ("path", "id") VALUES ('Person.horses.id', last_insert_rowid());
DROP TABLE "tmp_paths";"#
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
            r#"INSERT INTO "Person" DEFAULT VALUES;
REPLACE INTO "tmp_paths" ("path", "id") VALUES ('Person.id', last_insert_rowid());
INSERT INTO "Horse" DEFAULT VALUES;
REPLACE INTO "tmp_paths" ("path", "id") VALUES ('Person.horses.id', last_insert_rowid());
INSERT INTO "PersonsHorses" ("Horse.id", "Person.id") VALUES ((SELECT "id" FROM "tmp_paths" WHERE "path" = 'Person.horses.id'), (SELECT "id" FROM "tmp_paths" WHERE "path" = 'Person.id'));
INSERT INTO "Horse" DEFAULT VALUES;
REPLACE INTO "tmp_paths" ("path", "id") VALUES ('Person.horses.id', last_insert_rowid());
INSERT INTO "PersonsHorses" ("Horse.id", "Person.id") VALUES ((SELECT "id" FROM "tmp_paths" WHERE "path" = 'Person.horses.id'), (SELECT "id" FROM "tmp_paths" WHERE "path" = 'Person.id'));"#
        );
    }
}
