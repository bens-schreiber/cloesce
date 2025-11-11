use std::collections::HashMap;

use ast::NavigationPropertyKind::{ManyToMany, OneToMany};
use ast::{CidlType, Model, NamedTypedValue, NavigationProperty, NavigationPropertyKind};
use sea_query::{Alias, OnConflict, SimpleExpr, SqliteQueryBuilder, SubQueryStatement, Values};
use sea_query::{Expr, Query};
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

use crate::IncludeTree;
use crate::ModelMeta;
use crate::common::alias;

pub struct UpsertModel<'a> {
    meta: &'a ModelMeta,
    context: HashMap<String, Option<Value>>,
    acc: Vec<(String, Values)>,
}

#[derive(Serialize)]
pub struct UpsertResult {
    query: String,
    values: Vec<serde_json::Value>,
}

impl<'a> UpsertModel<'a> {
    /// Given a model, traverses topological order accumulating insert statements.
    ///
    /// Allows for empty primary keys and foreign keys, inferring the value is auto generated
    /// or context driven through navigation properties.
    ///
    /// If an ID is provided, on insertion conflict, defaults to updating the provided rows instead.
    ///
    /// Returns a string of SQL statements, or a descriptive error string.
    pub fn query(
        model_name: &str,
        meta: &'a ModelMeta,
        new_model: Map<String, Value>,
        include_tree: Option<&IncludeTree>,
    ) -> Result<Vec<UpsertResult>, String> {
        let mut stmts = {
            let mut generator = Self {
                meta,
                context: HashMap::default(),
                acc: Vec::default(),
            };

            generator.dfs(
                None,
                model_name,
                &new_model,
                include_tree,
                model_name.to_string(),
            )?;

            generator.acc
        };

        let select_root_id_stmt = {
            // unwrap: root model is guaranteed to exist if we've gotten this far
            let model = meta.get(model_name).unwrap();
            let root_id_path = format!("{}.{}", model.name, model.primary_key.name);

            // The root id is either a value, or a variable in the temp table.
            match new_model.get(&model.primary_key.name) {
                Some(value) => Query::select()
                    .expr_as(
                        match model.primary_key.cidl_type {
                            CidlType::Integer => Expr::val(value.as_i64().unwrap()),
                            CidlType::Real => Expr::val(value.as_f64().unwrap()),
                            _ => Expr::val(value.as_str().unwrap()),
                        },
                        alias(VARIABLES_TABLE_COL_ID),
                    )
                    .to_owned(),
                None => Query::select()
                    .from(alias(VARIABLES_TABLE_NAME))
                    .column(alias(VARIABLES_TABLE_COL_ID))
                    .and_where(
                        Expr::col(alias(VARIABLES_TABLE_COL_PATH)).eq(Expr::val(root_id_path)),
                    )
                    .to_owned(),
            }
            .build(SqliteQueryBuilder)
        };

        let remove_vars_stmt = VariablesTable::delete_all();

        stmts.push(select_root_id_stmt);
        stmts.push((remove_vars_stmt, Values(vec![])));

        // Convert from SeaQuery to serde_json
        let mut res = vec![];
        for (stmt, values) in stmts {
            let mut serde_values: Vec<Value> = vec![];
            for v in values {
                match v {
                    sea_query::Value::Int(Some(i)) => serde_values.push(Value::from(i)),
                    sea_query::Value::Int(None) => serde_values.push(Value::Null),
                    sea_query::Value::BigInt(Some(i)) => serde_values.push(Value::from(i)),
                    sea_query::Value::BigInt(None) => serde_values.push(Value::Null),
                    sea_query::Value::String(Some(s)) => serde_values.push(Value::String(*s)),
                    sea_query::Value::String(None) => serde_values.push(Value::Null),
                    sea_query::Value::Float(Some(f)) => serde_values.push(Value::from(f)),
                    sea_query::Value::Double(Some(d)) => serde_values.push(Value::from(d)),
                    _ => unimplemented!("Value type not implemented in upsert serde conversion"),
                }
            }

            res.push(UpsertResult {
                query: stmt,
                values: serde_values,
            });
        }

        Ok(res)
    }

    fn dfs(
        &mut self,
        parent_model_name: Option<&String>,
        model_name: &str,
        new_model: &Map<String, Value>,
        include_tree: Option<&IncludeTree>,
        path: String,
    ) -> Result<String, String> {
        let model = match self.meta.get(model_name) {
            Some(m) => m,
            None => return Err(format!("Unknown model {model_name}")),
        };

        let mut builder =
            UpsertBuilder::new(model_name, model.attributes.len(), &model.primary_key);

        // Primary key
        let pk = new_model.get(&model.primary_key.name);
        match pk {
            Some(val) => {
                builder.push_pk(val);
            }
            None if matches!(model.primary_key.cidl_type, CidlType::Integer) => {
                // Generated id
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

        let (one_to_ones, others): (Vec<_>, Vec<_>) = model
            .navigation_properties
            .iter()
            .partition(|n| matches!(n.kind, NavigationPropertyKind::OneToOne { .. }));

        // This table is dependent on it's 1:1 references, so they must be traversed before
        // table insertion (granted the include tree references them).
        let mut nav_ref_to_path = HashMap::new();
        if let Some(include_tree) = include_tree {
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

                nav_ref_to_path.insert(
                    reference,
                    self.dfs(
                        Some(&model.name),
                        &nav.model_name,
                        nav_model,
                        Some(nested_tree),
                        format!("{path}.{}", nav.var_name),
                    )?,
                );
            }
        }

        // Scalar attributes; attempt to retrieve FK's by value or context
        {
            // If this model is depends on another, it's dependency will have been inserted
            // before this model. Thus, it's parent pk has been inserted into the context under this path:
            let parent_id_path = parent_model_name.map(|p| {
                format!(
                    "{}.{}",
                    path.rsplit_once('.').map(|(h, _)| h).unwrap_or(&path),
                    self.meta.get(p).unwrap().primary_key.name
                )
            });

            for attr in &model.attributes {
                let path_key = nav_ref_to_path
                    .get(&attr.value.name)
                    .or(parent_id_path.as_ref());

                match (new_model.get(&attr.value.name), &attr.foreign_key_reference) {
                    (Some(value), _) => {
                        // A value was provided in `new_model`
                        builder.push_val(&attr.value.name, value, &attr.value.cidl_type)?;
                    }
                    (None, Some(_)) if path_key.is_some() => {
                        let path_key = path_key.unwrap();
                        let ctx = self.context.get(path_key).unwrap();
                        builder.push_val_ctx(
                            ctx,
                            &attr.value.name,
                            &attr.value.cidl_type,
                            path_key,
                        )?;
                    }
                    (None, None) if pk.is_some() => {
                        // PK is provided, but an attribute is missing. Assume
                        // this must be an update query.
                    }
                    _ => {
                        // This is an insert or upsert which cannot have missing attributes.
                        return Err(format!(
                            "Missing attribute {} on {}: {}",
                            attr.value.name,
                            model.name,
                            serde_json::to_string(&new_model).unwrap()
                        ));
                    }
                };
            }
        }

        // All dependencies haev been resolved by this point.
        let id_path = self.upsert_table(pk, &path, model, builder)?;

        // Traverse navigation properties, using the include tree as a guide
        if let Some(include_tree) = include_tree {
            for nav in others {
                let Some(Value::Object(nested_tree)) = include_tree.get(&nav.var_name) else {
                    continue;
                };

                match (&nav.kind, new_model.get(&nav.var_name)) {
                    (OneToMany { .. }, Some(Value::Array(nav_models))) => {
                        for nav_model in nav_models.iter().filter_map(|v| v.as_object()) {
                            self.dfs(
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
                            self.dfs(
                                Some(&model.name),
                                &nav.model_name,
                                nav_model,
                                Some(nested_tree),
                                format!("{path}.{}", nav.var_name),
                            )?;

                            self.insert_jct(&path, nav, unique_id, new_model, model)?;
                        }
                    }
                    _ => {
                        // Ignore
                    }
                }
            }
        }

        Ok(id_path)
    }

    /// Inserts a M:M junction table, consisting of the passed in models
    /// id and the navigation properties id.
    fn insert_jct(
        &mut self,
        path: &str,
        nav: &NavigationProperty,
        unique_id: &str,
        new_model: &Map<String, Value>,
        model: &Model,
    ) -> Result<(), String> {
        let nav_meta = self.meta.get(&nav.model_name).unwrap();
        let nav_pk = &nav_meta.primary_key;

        // Resolve both sides of the M:M relationship
        let pairs = [
            (
                format!("{}.{}", nav.model_name, nav_pk.name),
                &nav_pk.cidl_type,
                format!("{path}.{}.{}", nav.var_name, nav_pk.name),
            ),
            (
                format!("{}.{}", model.name, model.primary_key.name),
                &model.primary_key.cidl_type,
                format!("{path}.{}", model.primary_key.name),
            ),
        ];

        // Collect column/value pairs from context
        let mut entries = Vec::new();
        for (var_name, cidl_type, path_key) in pairs {
            let ctx_value = self.context.get(&path_key).ok_or(format!(
                "Expected many to many model to contain an ID, got {}",
                serde_json::to_string(new_model).unwrap(),
            ))?;

            let value = match ctx_value {
                Some(v) => validate_json_to_cidl(v, cidl_type, unique_id, &var_name)?,
                None => UpsertBuilder::value_from_ctx(&path_key),
            };
            entries.push((var_name, value));
        }

        // Sort columns to ensure deterministic SQL
        entries.sort_by(|a, b| a.0.cmp(&b.0));

        // Build INSERT
        let mut insert = Query::insert();
        insert
            .into_table(alias(unique_id))
            .on_conflict(OnConflict::new().do_nothing().to_owned())
            .columns(entries.iter().map(|(col, _)| alias(col)))
            .values_panic(entries.into_iter().map(|(_, val)| val));

        self.acc.push(insert.build(SqliteQueryBuilder));
        Ok(())
    }

    /// Inserts the [InsertBuilder], updating the graph context to include the tables id.
    ///
    /// Returns an error if foreign key values exist that can not be resolved.
    fn upsert_table(
        &mut self,
        pk: Option<&Value>,
        path: &str,
        model: &Model,
        builder: UpsertBuilder,
    ) -> Result<String, String> {
        self.acc.push(builder.build()?);
        let id_path = format!("{path}.{}", model.primary_key.name);

        // Add this models primary key to the context so dependents can resolve it
        match pk {
            None => {
                self.acc.push(VariablesTable::insert_rowid(&id_path));
                self.context.insert(id_path.clone(), None);
            }
            Some(val) => {
                self.context.insert(id_path.clone(), Some(val.clone()));
            }
        }

        Ok(id_path)
    }
}

const VARIABLES_TABLE_NAME: &str = "_cloesce_tmp";
const VARIABLES_TABLE_COL_PATH: &str = "path";
const VARIABLES_TABLE_COL_ID: &str = "id";

/// A cloesce-shipped table that for storing temporary SQL
/// values, needed for complex insertions.
///
/// Unfortunately, D1 supports only read-only CTE's, so this temp table is
/// the only option available to us.
///
/// See https://github.com/bens-schreiber/cloesce/blob/schreiber/orm-ctes/src/runtime/src/methods/insert.rs
/// for a CTE based soltuion if that ever changes.
struct VariablesTable;
impl VariablesTable {
    fn delete_all() -> String {
        Query::delete()
            .from_table(alias(VARIABLES_TABLE_NAME))
            .to_string(SqliteQueryBuilder)
    }

    fn insert_rowid(path: &str) -> (String, Values) {
        Query::insert()
            .into_table(alias(VARIABLES_TABLE_NAME))
            .columns(vec![alias("path"), alias("id")])
            .values_panic(vec![
                Expr::val(path).into(),
                Expr::cust("last_insert_rowid()"),
            ])
            .replace()
            .build(SqliteQueryBuilder)
    }
}

struct UpsertBuilder<'a> {
    model_name: &'a str,
    scalar_len: usize,
    cols: Vec<Alias>,
    vals: Vec<SimpleExpr>,
    pk_val: Option<&'a Value>,
    pk_ntv: &'a NamedTypedValue,
}

impl<'a> UpsertBuilder<'a> {
    fn new(
        model_name: &'a str,
        scalar_len: usize,
        pk_ntv: &'a NamedTypedValue,
    ) -> UpsertBuilder<'a> {
        Self {
            scalar_len,
            model_name,
            pk_ntv,
            cols: Vec::default(),
            vals: Vec::default(),
            pk_val: None,
        }
    }

    /// Sets the primary key value
    fn push_pk(&mut self, val: &'a Value) {
        self.pk_val = Some(val);
    }

    /// Adds a column and value to the insert statement.
    ///
    /// Returns an error if the value does not match the meta type.
    fn push_val(
        &mut self,
        var_name: &str,
        value: &Value,
        cidl_type: &CidlType,
    ) -> Result<(), String> {
        self.cols.push(alias(var_name));
        let val = validate_json_to_cidl(value, cidl_type, self.model_name, var_name)?;
        self.vals.push(val);
        Ok(())
    }

    /// Adds a column and value using the graph context.
    fn push_val_ctx(
        &mut self,
        ctx: &Option<Value>,
        var_name: &str,
        cidl_type: &CidlType,
        path: &str,
    ) -> Result<(), String> {
        match ctx {
            None => {
                self.cols.push(alias(var_name));
                self.vals.push(Self::value_from_ctx(path));
            }
            Some(v) => {
                self.push_val(var_name, v, cidl_type)?;
            }
        }
        Ok(())
    }

    fn value_from_ctx(path: &str) -> SimpleExpr {
        let subq = SubQueryStatement::SelectStatement(
            Query::select()
                .from(alias(VARIABLES_TABLE_NAME))
                .column(alias(VARIABLES_TABLE_COL_ID))
                .and_where(Expr::col(alias(VARIABLES_TABLE_COL_PATH)).eq(path))
                .to_owned(),
        );

        SimpleExpr::SubQuery(None, Box::new(subq))
    }

    /// Creates a SQL query, being either an update only, insert only, or upsert.
    fn build(self) -> Result<(String, Values), String> {
        let pk_expr = self
            .pk_val
            .map(|v| {
                validate_json_to_cidl(
                    v,
                    &self.pk_ntv.cidl_type,
                    self.model_name,
                    &self.pk_ntv.name,
                )
            })
            .transpose()?;

        // Attributes are missing, but there is a PK. This must be an update query.
        if self.cols.len() < self.scalar_len {
            let Some(pk_expr) = pk_expr else {
                unreachable!("An attribute for an upsert is missing");
            };

            let mut update = Query::update();
            update
                .table(alias(self.model_name))
                .values(self.cols.into_iter().zip(self.vals))
                .and_where(Expr::col(alias(&self.pk_ntv.name)).eq(pk_expr));

            return Ok(update.build(SqliteQueryBuilder));
        }

        let mut insert = {
            let mut insert = Query::insert();
            insert.into_table(alias(self.model_name));

            let mut cols = self.cols.clone();
            let mut vals = self.vals.clone();
            if let Some(pk_expr) = pk_expr {
                cols.push(alias(&self.pk_ntv.name));
                vals.push(pk_expr);
            }

            insert
                .columns(cols)
                .values_panic(vals)
                .or_default_values()
                .to_owned()
        };

        // Some id exists, and some column is being inserted, so this must be an upsert (either insert or update).
        if self.pk_val.is_some() && !self.cols.is_empty() {
            insert.on_conflict(
                OnConflict::column(alias(&self.pk_ntv.name))
                    .update_columns(self.cols)
                    .to_owned(),
            );
        }

        Ok(insert.build(SqliteQueryBuilder))
    }
}

/// Validates that a JSON input follows the CIDL type, returning
/// a SeaQuery [SimpleExpr] value
fn validate_json_to_cidl(
    value: &Value,
    cidl_type: &CidlType,
    model_name: &str,
    attr_name: &str,
) -> Result<SimpleExpr, String> {
    if matches!(cidl_type, CidlType::Nullable(_)) && value.is_null() {
        return Ok(SimpleExpr::Custom("null".into()));
    }

    match cidl_type.root_type() {
        CidlType::Integer | CidlType::Boolean => {
            if !matches!(value, Value::Number(_)) {
                return Err(format!(
                    "Expected an integer type for {}.{}",
                    model_name, attr_name
                ));
            }

            Ok(Expr::val(value.as_i64().unwrap()).into())
        }
        CidlType::Real => {
            if !matches!(value, Value::Number(_)) {
                return Err(format!(
                    "Expected an real type for {}.{}",
                    model_name, attr_name
                ));
            }

            Ok(Expr::val(value.as_f64().unwrap()).into())
        }
        CidlType::Text | CidlType::DateIso => {
            if !matches!(value, Value::String(_)) {
                return Err(format!(
                    "Expected an real type for {}.{}",
                    model_name, attr_name
                ));
            }

            Ok(Expr::val(value.as_str().unwrap()).into())
        }
        _ => {
            unreachable!("Invalid CIDL");
        }
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use ast::{CidlType, NavigationPropertyKind, builder::ModelBuilder};
    use serde_json::{Value, json};
    use sqlx::SqlitePool;

    use crate::{common::test_sql, expected_str, upsert::UpsertModel};

    #[sqlx::test]
    fn upsert_scalar_model(db: SqlitePool) {
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

        let mut meta = HashMap::new();
        meta.insert(ast_model.name.clone(), ast_model);

        // Act
        let res = UpsertModel::query("Horse", &meta, new_model.as_object().unwrap().clone(), None)
            .unwrap();

        // Assert
        assert_eq!(res.len(), 3);

        let res1 = &res[0];
        expected_str!(
            res1.query,
            r#"INSERT INTO "Horse" ("color", "age", "address", "id") VALUES (?, ?, null, ?)"#
        );
        expected_str!(
            res1.query,
            r#"ON CONFLICT ("id") DO UPDATE SET "color" = "excluded"."color", "age" = "excluded"."age", "address" = "excluded"."address""#
        );
        assert_eq!(
            *res1.values,
            vec![Value::from("brown"), Value::from(7i64), Value::from(1i64)]
        );

        let res2 = &res[1];
        expected_str!(res2.query, r#"SELECT ? AS "id""#);
        assert_eq!(*res2.values, vec![Value::from(1i64)]);

        let res3 = &res[2];
        expected_str!(res3.query, r#"DELETE FROM "_cloesce_tmp""#);
        assert_eq!(res3.values.len(), 0);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn update_scalar_model(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Horse")
            .id()
            .attribute("color", CidlType::Text, None)
            .attribute("age", CidlType::Integer, None)
            .attribute("address", CidlType::nullable(CidlType::Text), None)
            .build();

        let new_model = json!({
            "id": 1,
            "age": 7,
            "address": null
        });

        let mut meta = HashMap::new();
        meta.insert(ast_model.name.clone(), ast_model);

        // Act
        let res = UpsertModel::query("Horse", &meta, new_model.as_object().unwrap().clone(), None)
            .unwrap();

        // Assert
        assert_eq!(res.len(), 3);

        let res1 = &res[0];
        expected_str!(
            res1.query,
            r#"UPDATE "Horse" SET "age" = ?, "address" = null WHERE "id" = ?"#
        );
        assert_eq!(*res1.values, vec![Value::from(7), Value::from(1)]);

        let res2 = &res[1];
        expected_str!(res2.query, r#"SELECT ? AS "id""#);
        assert_eq!(*res2.values, vec![Value::from(1i64)]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn nav_props_no_include_tree(db: SqlitePool) {
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

        let mut meta = HashMap::new();
        meta.insert(ast_horse.name.clone(), ast_horse);
        meta.insert(ast_person.name.clone(), ast_person);

        // Act
        let res = UpsertModel::query(
            "Person",
            &meta,
            new_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 3);

        let res1 = &res[0];
        expected_str!(
            res1.query,
            r#"INSERT INTO "Person" ("horseId", "id") VALUES (?, ?)"#
        );
        assert_eq!(*res1.values, vec![1i64, 1i64]);

        let res2 = &res[1];
        expected_str!(res2.query, r#"SELECT ? AS "id""#);
        assert_eq!(*res2.values, vec![1i64]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .unwrap_err();
    }

    #[sqlx::test]
    async fn one_to_one(db: SqlitePool) {
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

        let mut meta = HashMap::new();
        meta.insert(ast_horse.name.clone(), ast_horse);
        meta.insert(ast_person.name.clone(), ast_person);

        // Act
        let res = UpsertModel::query(
            "Person",
            &meta,
            new_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        let res1 = &res[0];
        expected_str!(res1.query, r#"INSERT INTO "Horse" ("id") VALUES (?)"#);
        assert_eq!(*res1.values, vec![1]);

        let res2 = &res[1];
        expected_str!(
            res2.query,
            r#"INSERT INTO "Person" ("horseId", "id") VALUES (?, ?) ON CONFLICT ("id") DO UPDATE SET "horseId" = "excluded"."horseId""#
        );
        assert_eq!(*res2.values, vec![1, 1]);

        let res3 = &res[2];
        expected_str!(res3.query, r#"SELECT ? AS "id""#);
        assert_eq!(*res3.values, vec![1]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn one_to_many(db: SqlitePool) {
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

        let mut meta = HashMap::new();
        meta.insert(ast_horse.name.clone(), ast_horse);
        meta.insert(ast_person.name.clone(), ast_person);

        // Act
        let res = UpsertModel::query(
            "Person",
            &meta,
            new_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 6);

        let res1 = &res[0];
        expected_str!(res1.query, r#"INSERT INTO "Person" ("id") VALUES (?)"#);
        assert_eq!(*res1.values, vec![1]);

        let res2 = &res[1];
        expected_str!(
            res2.query,
            r#"INSERT INTO "Horse" ("personId", "id") VALUES (?, ?) ON CONFLICT ("id") DO UPDATE SET "personId" = "excluded"."personId""#
        );
        assert_eq!(*res2.values, vec![1, 1]);

        let res3 = &res[2];
        expected_str!(
            res3.query,
            r#"INSERT INTO "Horse" ("personId", "id") VALUES (?, ?) ON CONFLICT ("id") DO UPDATE SET "personId" = "excluded"."personId""#
        );
        assert_eq!(*res3.values, vec![1, 2]);

        let res4 = &res[3];
        expected_str!(
            res4.query,
            r#"INSERT INTO "Horse" ("personId", "id") VALUES (?, ?) ON CONFLICT ("id") DO UPDATE SET "personId" = "excluded"."personId""#
        );
        assert_eq!(*res4.values, vec![1, 3]);

        let res5 = &res[4];
        expected_str!(res5.query, r#"SELECT ? AS "id""#);
        assert_eq!(*res5.values, vec![1]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn many_to_many(db: SqlitePool) {
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
        let ast_horse = ModelBuilder::new("Horse")
            .nav_p(
                "persons",
                "Person",
                NavigationPropertyKind::ManyToMany {
                    unique_id: "PersonsHorses".to_string(),
                },
            )
            .id()
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
            ]
        });

        let include_tree = json!({
            "horses": {}
        });

        let mut meta = HashMap::new();
        meta.insert(ast_horse.name.clone(), ast_horse);
        meta.insert(ast_person.name.clone(), ast_person);

        // Act
        let res = UpsertModel::query(
            "Person",
            &meta,
            new_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 7);

        let res1 = &res[0];
        expected_str!(res1.query, r#"INSERT INTO "Person" ("id") VALUES (?)"#);
        assert_eq!(*res1.values, vec![1]);

        let res2 = &res[1];
        expected_str!(res2.query, r#"INSERT INTO "Horse" ("id") VALUES (?)"#);
        assert_eq!(*res2.values, vec![1]);

        let res3 = &res[2];
        expected_str!(
            res3.query,
            r#"INSERT INTO "PersonsHorses" ("Horse.id", "Person.id") VALUES (?, ?) ON CONFLICT  DO NOTHING"#
        );
        assert_eq!(*res3.values, vec![1, 1]);

        let res4 = &res[3];
        expected_str!(res4.query, r#"INSERT INTO "Horse" ("id") VALUES (?)"#);
        assert_eq!(*res4.values, vec![2]);

        let res5 = &res[4];
        expected_str!(
            res5.query,
            r#"INSERT INTO "PersonsHorses" ("Horse.id", "Person.id") VALUES (?, ?) ON CONFLICT  DO NOTHING"#
        );
        assert_eq!(*res5.values, vec![2, 1]);

        let res6 = &res[5];
        expected_str!(res6.query, r#"SELECT ? AS "id""#);
        assert_eq!(*res6.values, vec![1]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn topological_ordering_is_correct(db: SqlitePool) {
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
        let res = UpsertModel::query(
            "Person",
            &meta,
            new_model.as_object().unwrap().clone(),
            Some(&include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 6);

        let inserts: Vec<_> = res
            .iter()
            .filter(|stmt| stmt.query.starts_with("INSERT"))
            .collect();

        assert!(
            inserts[0].query.contains("INSERT INTO \"Horse\""),
            "Expected Horse inserted first, got {}",
            inserts[0].query
        );

        assert!(
            inserts[1].query.contains("INSERT INTO \"Award\""),
            "Expected Award inserted third, got {}",
            inserts[1].query
        );

        assert!(
            inserts[2].query.contains("INSERT INTO \"Award\""),
            "Expected another Award insert, got {}",
            inserts[2].query
        );

        assert!(
            inserts[3].query.contains("INSERT INTO \"Person\""),
            "Expected Person inserted second, got {}",
            inserts[3].query
        );

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn insert_missing_pk_autogenerates(db: SqlitePool) {
        // Arrange
        let person = ModelBuilder::new("Person").id().build();
        let mut meta = std::collections::HashMap::new();
        meta.insert(person.name.clone(), person);

        let new_person = json!({});

        // Act
        let res = UpsertModel::query(
            "Person",
            &meta,
            new_person.as_object().unwrap().clone(),
            None,
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 4);

        let res1 = &res[0];
        expected_str!(res1.query, r#"INSERT INTO "Person" DEFAULT VALUES"#);
        assert_eq!(res1.values.len(), 0);

        let res2 = &res[1];
        expected_str!(
            res2.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*res2.values, vec!["Person.id"]);

        let res3 = &res[2];
        expected_str!(
            res3.query,
            r#"SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?"#
        );
        assert_eq!(*res3.values, vec!["Person.id"]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn insert_missing_one_to_one_fk_autogenerates(db: SqlitePool) {
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
        let res = UpsertModel::query(
            "Person",
            &meta,
            new_person.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap()),
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 6);

        let res1 = &res[0];
        expected_str!(res1.query, r#"INSERT INTO "Horse" DEFAULT VALUES"#);
        assert_eq!(res1.values.len(), 0);

        let res2 = &res[1];
        expected_str!(
            res2.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*res2.values, vec!["Person.horse.id"]);

        let res3 = &res[2];
        expected_str!(
            res3.query,
            r#"INSERT INTO "Person" ("horseId") VALUES ((SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?))"#
        );
        assert_eq!(*res3.values, vec!["Person.horse.id"]);

        let res4 = &res[3];
        expected_str!(
            res4.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*res4.values, vec!["Person.id"]);

        let res5 = &res[4];
        expected_str!(
            res5.query,
            r#"SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?"#
        );
        assert_eq!(*res5.values, vec!["Person.id"]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn insert_missing_one_to_many_fk_autogenerates(db: SqlitePool) {
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
        let res = UpsertModel::query(
            "Person",
            &meta,
            new_person.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap()),
        )
        .unwrap();

        // Assert

        let res1 = &res[0];
        expected_str!(res1.query, r#"INSERT INTO "Person" DEFAULT VALUES"#);
        assert_eq!(res1.values.len(), 0);

        let res2 = &res[1];
        expected_str!(
            res2.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*res2.values, vec!["Person.id"]);

        let res3 = &res[2];
        expected_str!(
            res3.query,
            r#"INSERT INTO "Horse" ("personId") VALUES ((SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?))"#
        );
        assert_eq!(*res3.values, vec!["Person.id"]);

        let res4 = &res[3];
        expected_str!(
            res4.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*res4.values, vec!["Person.horses.id"]);

        let res5 = &res[4];
        expected_str!(
            res5.query,
            r#"SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?"#
        );
        assert_eq!(*res5.values, vec!["Person.id"]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn insert_missing_many_to_many_pk_fk_autogenerates(db: SqlitePool) {
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

        let horse = ModelBuilder::new("Horse")
            .nav_p(
                "persons",
                "Person",
                NavigationPropertyKind::ManyToMany {
                    unique_id: "PersonsHorses".to_string(),
                },
            )
            .id()
            .build();

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
        let res = UpsertModel::query(
            "Person",
            &meta,
            new_person.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap()),
        )
        .unwrap();

        // Assert

        let res1 = &res[0];
        expected_str!(res1.query, r#"INSERT INTO "Person" DEFAULT VALUES"#);
        assert_eq!(res1.values.len(), 0);

        let res2 = &res[1];
        expected_str!(
            res2.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*res2.values, vec!["Person.id"]);

        let res3 = &res[2];
        expected_str!(res3.query, r#"INSERT INTO "Horse" DEFAULT VALUES"#);
        assert_eq!(res3.values.len(), 0);

        let res4 = &res[3];
        expected_str!(
            res4.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*res4.values, vec!["Person.horses.id"]);

        let res5 = &res[4];
        expected_str!(
            res5.query,
            r#"INSERT INTO "PersonsHorses" ("Horse.id", "Person.id") VALUES ((SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?), (SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?)) ON CONFLICT  DO NOTHING"#
        );
        assert_eq!(*res5.values, vec!["Person.horses.id", "Person.id"]);

        let res6 = &res[5];
        expected_str!(res6.query, r#"INSERT INTO "Horse" DEFAULT VALUES"#);
        assert_eq!(res6.values.len(), 0);

        let res7 = &res[6];
        expected_str!(
            res7.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*res7.values, vec!["Person.horses.id"]);

        let res8 = &res[7];
        expected_str!(
            res8.query,
            r#"INSERT INTO "PersonsHorses" ("Horse.id", "Person.id") VALUES ((SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?), (SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?)) ON CONFLICT  DO NOTHING"#
        );
        assert_eq!(*res8.values, vec!["Person.horses.id", "Person.id"]);

        let res9 = &res[8];
        expected_str!(
            res9.query,
            r#"SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?"#
        );
        assert_eq!(*res9.values, vec!["Person.id"]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }
}
