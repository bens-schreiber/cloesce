use std::collections::HashMap;

use ast::{CidlType, Model, NamedTypedValue, NavigationProperty, NavigationPropertyKind, fail};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use sea_query::{Alias, OnConflict, SimpleExpr, SqliteQueryBuilder, SubQueryStatement, Values};
use sea_query::{Expr, Query};
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

use crate::ModelMeta;
use crate::methods::json::as_json;
use crate::methods::{OrmErrorKind, alias};
use crate::{IncludeTreeJson, ensure};

use super::Result;

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
        include_tree: Option<IncludeTreeJson>,
    ) -> Result<Vec<UpsertResult>> {
        let include_tree = include_tree.unwrap_or_default();
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
                &include_tree,
                model_name.to_string(),
            )?;

            generator.acc
        };

        let select_json = as_json(model_name, Some(include_tree), meta)?;
        let select_root_model = {
            // unwrap: root model is guaranteed to exist if we've gotten this far
            let model = meta.get(model_name).unwrap();
            let pk_col = &model.primary_key.as_ref().unwrap().name;

            let mut select = Query::select();
            select
                .expr(Expr::cust(&select_json))
                .from(alias(model_name))
                .and_where(Expr::col(alias(pk_col)).eq(match new_model.get(pk_col) {
                    Some(value) => validate_json_to_cidl(
                        value,
                        &model.primary_key.as_ref().unwrap().cidl_type,
                        model_name,
                        pk_col,
                    )?,
                    None => UpsertBuilder::value_from_ctx(&format!("{}.{}", model.name, pk_col)),
                }));

            select.build(SqliteQueryBuilder)
        };

        stmts.push(select_root_model);
        stmts.push((VariablesTable::delete_all(), Values(vec![])));

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
        include_tree: &IncludeTreeJson,
        path: String,
    ) -> Result<String> {
        let model = match self.meta.get(model_name) {
            Some(m) => m,
            None => fail!(OrmErrorKind::UnknownModel, "{}", model_name),
        };
        if model.primary_key.is_none() {
            fail!(
                OrmErrorKind::ModelMissingD1,
                "Model '{}' is not a D1 model.",
                model_name
            )
        }

        let mut builder = UpsertBuilder::new(
            model_name,
            model.columns.len(),
            model.primary_key.as_ref().unwrap(),
        );

        // Primary key
        let pk = new_model.get(&model.primary_key.as_ref().unwrap().name);
        match pk {
            Some(val) => {
                builder.push_pk(val);
            }
            None if matches!(
                model.primary_key.as_ref().unwrap().cidl_type,
                CidlType::Integer
            ) =>
            {
                // Generated id
            }
            _ => {
                fail!(
                    OrmErrorKind::MissingPrimaryKey,
                    "{}.{}",
                    model.name,
                    serde_json::to_string(new_model).unwrap()
                );
            }
        };

        let (one_to_ones, others): (Vec<_>, Vec<_>) = model
            .navigation_properties
            .iter()
            .partition(|n| matches!(n.kind, NavigationPropertyKind::OneToOne { .. }));

        // This table is dependent on it's 1:1 references, so they must be traversed before
        // table insertion (granted the include tree references them).
        let mut nav_ref_to_path = HashMap::new();
        for nav in one_to_ones {
            let Some(Value::Object(nested_tree)) = include_tree.get(&nav.var_name) else {
                continue;
            };
            let Some(Value::Object(nav_model)) = new_model.get(&nav.var_name) else {
                continue;
            };
            let NavigationPropertyKind::OneToOne { column_reference } = &nav.kind else {
                continue;
            };

            // Recursively handle nested inserts
            nav_ref_to_path.insert(
                column_reference,
                self.dfs(
                    Some(&model.name),
                    &nav.model_reference,
                    nav_model,
                    nested_tree,
                    format!("{path}.{}", nav.var_name),
                )?,
            );
        }

        // Scalar attributes; attempt to retrieve FK's by value or context
        {
            // If this model is depends on another, it's dependency will have been inserted
            // before this model. Thus, it's parent pk has been inserted into the context under this path:
            let parent_id_path = parent_model_name.map(|p| {
                format!(
                    "{}.{}",
                    path.rsplit_once('.').map(|(h, _)| h).unwrap_or(&path),
                    self.meta.get(p).unwrap().primary_key.as_ref().unwrap().name
                )
            });

            for attr in &model.columns {
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
                        fail!(
                            OrmErrorKind::MissingAttribute,
                            "{}.{}: {}",
                            model.name,
                            attr.value.name,
                            serde_json::to_string(&new_model).unwrap()
                        );
                    }
                };
            }
        }

        // All dependencies haev been resolved by this point.
        let id_path = self.upsert_table(pk, &path, model, builder)?;

        // Traverse navigation properties, using the include tree as a guide
        for nav in others {
            let Some(Value::Object(nested_tree)) = include_tree.get(&nav.var_name) else {
                continue;
            };

            match (&nav.kind, new_model.get(&nav.var_name)) {
                (NavigationPropertyKind::OneToMany { .. }, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models.iter().filter_map(|v| v.as_object()) {
                        self.dfs(
                            Some(&model.name),
                            &nav.model_reference,
                            nav_model,
                            nested_tree,
                            format!("{path}.{}", nav.var_name),
                        )?;
                    }
                }
                (NavigationPropertyKind::ManyToMany, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models.iter().filter_map(|v| v.as_object()) {
                        self.dfs(
                            Some(&model.name),
                            &nav.model_reference,
                            nav_model,
                            nested_tree,
                            format!("{path}.{}", nav.var_name),
                        )?;

                        let m2m_table_name = nav.many_to_many_table_name(model_name);
                        self.insert_jct(&path, nav, &m2m_table_name, model)?;
                    }
                }
                _ => {
                    // Ignore
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
        model: &Model,
    ) -> Result<()> {
        let nav_meta = self.meta.get(&nav.model_reference).unwrap();
        let nav_pk = nav_meta.primary_key.as_ref().unwrap();
        let model_pk = model.primary_key.as_ref().unwrap();

        // Resolve both sides of the M:M relationship
        let pairs = [
            (
                format!("{}.{}", nav.model_reference, nav_pk.name),
                &nav_pk.cidl_type,
                format!("{path}.{}.{}", nav.var_name, nav_pk.name),
            ),
            (
                format!("{}.{}", model.name, model_pk.name),
                &model_pk.cidl_type,
                format!("{path}.{}", model_pk.name),
            ),
        ];

        // Collect column/value pairs from context
        let mut entries = Vec::new();
        for (i, (var_name, cidl_type, path_key)) in pairs.iter().enumerate() {
            let value = match self.context.get(path_key).cloned().flatten() {
                Some(v) => validate_json_to_cidl(&v, cidl_type, unique_id, var_name)?,
                None => UpsertBuilder::value_from_ctx(path_key),
            };

            // m2m tables always have "left" and "right" columns
            let col_name = if i == 0 { "left" } else { "right" };

            entries.push((col_name.to_string(), value));
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
    ) -> Result<String> {
        self.acc.push(builder.build()?);
        let id_path = format!("{path}.{}", model.primary_key.as_ref().unwrap().name);

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
    fn push_val(&mut self, var_name: &str, value: &Value, cidl_type: &CidlType) -> Result<()> {
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
    ) -> Result<()> {
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

    /// Creates a SQL query, being either an update, insert, or upsert.
    fn build(self) -> Result<(String, Values)> {
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
) -> Result<SimpleExpr> {
    if matches!(cidl_type, CidlType::Nullable(_)) && value.is_null() {
        return Ok(SimpleExpr::Custom("null".into()));
    }

    match cidl_type.root_type() {
        CidlType::Integer | CidlType::Boolean => {
            ensure!(
                matches!(value, Value::Number(_)),
                OrmErrorKind::TypeMismatch,
                "{}.{}",
                model_name,
                attr_name
            );

            Ok(Expr::val(value.as_i64().unwrap()).into())
        }
        CidlType::Real => {
            ensure!(
                matches!(value, Value::Number(_)),
                OrmErrorKind::TypeMismatch,
                "{}.{}",
                model_name,
                attr_name
            );

            Ok(Expr::val(value.as_f64().unwrap()).into())
        }
        CidlType::Text | CidlType::DateIso => {
            ensure!(
                matches!(value, Value::String(_)),
                OrmErrorKind::TypeMismatch,
                "{}.{}",
                model_name,
                attr_name
            );

            Ok(Expr::val(value.as_str().unwrap()).into())
        }
        CidlType::Blob => match value {
            // Base64 string
            Value::String(b64) => {
                let bytes = match BASE64_STANDARD.decode(b64) {
                    Ok(b) => b,
                    Err(_) => fail!(OrmErrorKind::TypeMismatch, "{}.{}", model_name, attr_name),
                };

                Ok(bytes_to_sqlite(&bytes))
            }

            // Byte array
            Value::Array(inner) => {
                let mut bytes = Vec::with_capacity(inner.len());
                for v in inner {
                    let n = match v.as_u64() {
                        Some(n) => n,
                        None => fail!(OrmErrorKind::TypeMismatch, "{}.{}", model_name, attr_name),
                    };

                    if n > 255 {
                        fail!(OrmErrorKind::TypeMismatch, "{}.{}", model_name, attr_name);
                    }
                    bytes.push(n as u8);
                }

                Ok(bytes_to_sqlite(&bytes))
            }
            _ => fail!(OrmErrorKind::TypeMismatch, "{}.{}", model_name, attr_name),
        },
        _ => {
            unreachable!("Invalid CIDL");
        }
    }
}

/// Convert a byte array to a sqlite hex string suitable
/// for [CidlType::Blob] columns
fn bytes_to_sqlite(bytes: &[u8]) -> SimpleExpr {
    let hex = bytes
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<String>();
    SimpleExpr::Custom(format!("X'{}'", hex))
}

#[cfg(test)]
mod test {
    use std::{collections::HashMap, str::FromStr as _};

    use ast::{CidlType, NavigationPropertyKind};
    use generator_test::{ModelBuilder, expected_str};
    use serde_json::{Value, json};
    use sqlx::{Row, SqlitePool};

    use crate::methods::{test_sql, upsert::UpsertModel};

    #[sqlx::test]
    async fn upsert_scalar_model(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Horse")
            .id_pk()
            .col("color", CidlType::Text, None)
            .col("age", CidlType::Integer, None)
            .col("address", CidlType::nullable(CidlType::Text), None)
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

        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "Horse" ("color", "age", "address", "id") VALUES (?, ?, null, ?)"#
        );
        expected_str!(
            stmt1.query,
            r#"ON CONFLICT ("id") DO UPDATE SET "color" = "excluded"."color", "age" = "excluded"."age", "address" = "excluded"."address""#
        );
        assert_eq!(
            *stmt1.values,
            vec![Value::from("brown"), Value::from(7i64), Value::from(1i64)]
        );

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"WHERE "id" = ?"#);
        assert_eq!(*stmt2.values, vec![Value::from(1i64)]);

        let stmt3 = &res[2];
        expected_str!(stmt3.query, r#"DELETE FROM "_cloesce_tmp""#);
        assert_eq!(stmt3.values.len(), 0);

        let results = test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let value = Value::from_str(results[1][0].try_get(0).unwrap()).unwrap();
        assert_eq!(value, Value::Array(vec![new_model]));
    }

    #[sqlx::test]
    async fn update_scalar_model(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Horse")
            .id_pk()
            .col("color", CidlType::Text, None)
            .col("age", CidlType::Integer, None)
            .col("address", CidlType::nullable(CidlType::Text), None)
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

        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"UPDATE "Horse" SET "age" = ?, "address" = null WHERE "id" = ?"#
        );
        assert_eq!(*stmt1.values, vec![Value::from(7), Value::from(1)]);

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"WHERE "id" = ?"#);
        assert_eq!(*stmt2.values, vec![Value::from(1i64)]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn upsert_blob_b64(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Picture")
            .id_pk()
            .col("metadata", CidlType::Text, None)
            .col("blob", CidlType::Blob, None)
            .build();

        let mut meta = HashMap::new();
        meta.insert(ast_model.name.clone(), ast_model);

        let mut new_model = json!({
            "id": 1,
            "metadata": "meta",
            "blob": "aGVsbG8gd29ybGQ="
        });

        // Act
        let res = UpsertModel::query(
            "Picture",
            &meta,
            new_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 3);

        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "Picture" ("metadata", "blob", "id") VALUES (?, X'68656C6C6F20776F726C64', ?) ON CONFLICT ("id") DO UPDATE SET "metadata" = "excluded"."metadata", "blob" = "excluded"."blob""#
        );
        assert_eq!(*stmt1.values, vec![Value::from("meta"), Value::from(1i64),]);

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"WHERE "id" = ?"#);
        assert_eq!(*stmt2.values, vec![Value::from(1i64)]);

        let results = test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let value = Value::from_str(results[1][0].try_get(0).unwrap()).unwrap();
        new_model["blob"] = "68656C6C6F20776F726C64".into();
        assert_eq!(value, Value::Array(vec![new_model]));
    }

    #[sqlx::test]
    async fn upsert_blob_u8_arr(db: SqlitePool) {
        // Arrange
        let ast_model = ModelBuilder::new("Picture")
            .id_pk()
            .col("metadata", CidlType::Text, None)
            .col("blob", CidlType::Blob, None)
            .build();

        let mut meta = HashMap::new();
        meta.insert(ast_model.name.clone(), ast_model);

        let new_model = json!({
            "id": 1,
            "metadata": "meta",
            "blob": [
                104, 101, 108, 108, 111, // hello
                32,                      // space
                119, 111, 114, 108, 100  // world
            ]
        });

        // Act
        let res = UpsertModel::query(
            "Picture",
            &meta,
            new_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 3);

        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "Picture" ("metadata", "blob", "id") VALUES (?, X'68656C6C6F20776F726C64', ?) ON CONFLICT ("id") DO UPDATE SET "metadata" = "excluded"."metadata", "blob" = "excluded"."blob""#
        );
        assert_eq!(*stmt1.values, vec![Value::from("meta"), Value::from(1i64),]);

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"WHERE "id" = ?"#);
        assert_eq!(*stmt2.values, vec![Value::from(1i64)]);

        test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn one_to_one(db: SqlitePool) {
        // Arrange
        let ast_person = ModelBuilder::new("Person")
            .id_pk()
            .col("horseId", CidlType::Integer, Some("Horse".into()))
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    column_reference: "horseId".to_string(),
                },
            )
            .build();
        let ast_horse = ModelBuilder::new("Horse").id_pk().build();

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
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        let stmt1 = &res[0];
        expected_str!(stmt1.query, r#"INSERT INTO "Horse" ("id") VALUES (?)"#);
        assert_eq!(*stmt1.values, vec![1]);

        let stmt2 = &res[1];
        expected_str!(
            stmt2.query,
            r#"INSERT INTO "Person" ("horseId", "id") VALUES (?, ?) ON CONFLICT ("id") DO UPDATE SET "horseId" = "excluded"."horseId""#
        );
        assert_eq!(*stmt2.values, vec![1, 1]);

        let stmt3 = &res[2];
        expected_str!(stmt3.query, r#"WHERE "id" = ?"#);
        assert_eq!(*stmt3.values, vec![1]);

        let results = test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let value = Value::from_str(results[2][0].try_get(0).unwrap()).unwrap();
        assert_eq!(value, Value::Array(vec![new_model]));
    }

    #[sqlx::test]
    async fn one_to_many(db: SqlitePool) {
        // Arrange
        let ast_person = ModelBuilder::new("Person")
            .id_pk()
            .nav_p(
                "horses",
                "Horse",
                NavigationPropertyKind::OneToMany {
                    column_reference: "personId".to_string(),
                },
            )
            .build();
        let ast_horse = ModelBuilder::new("Horse")
            .id_pk()
            .col("personId", CidlType::Integer, Some("Person".into()))
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
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 6);

        let stmt1 = &res[0];
        expected_str!(stmt1.query, r#"INSERT INTO "Person" ("id") VALUES (?)"#);
        assert_eq!(*stmt1.values, vec![1]);

        let stmt2 = &res[1];
        expected_str!(
            stmt2.query,
            r#"INSERT INTO "Horse" ("personId", "id") VALUES (?, ?) ON CONFLICT ("id") DO UPDATE SET "personId" = "excluded"."personId""#
        );
        assert_eq!(*stmt2.values, vec![1, 1]);

        let stmt3 = &res[2];
        expected_str!(
            stmt3.query,
            r#"INSERT INTO "Horse" ("personId", "id") VALUES (?, ?) ON CONFLICT ("id") DO UPDATE SET "personId" = "excluded"."personId""#
        );
        assert_eq!(*stmt3.values, vec![1, 2]);

        let stmt4 = &res[3];
        expected_str!(
            stmt4.query,
            r#"INSERT INTO "Horse" ("personId", "id") VALUES (?, ?) ON CONFLICT ("id") DO UPDATE SET "personId" = "excluded"."personId""#
        );
        assert_eq!(*stmt4.values, vec![1, 3]);

        let stmt5 = &res[4];
        expected_str!(stmt5.query, r#"WHERE "id" = ?"#);
        assert_eq!(*stmt5.values, vec![1]);

        let results = test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let value = Value::from_str(results[4][0].try_get(0).unwrap()).unwrap();
        assert_eq!(value, Value::Array(vec![new_model]));
    }

    #[sqlx::test]
    async fn many_to_many(db: SqlitePool) {
        // Arrange
        let ast_person = ModelBuilder::new("Person")
            .id_pk()
            .nav_p("horses", "Horse", NavigationPropertyKind::ManyToMany)
            .build();
        let ast_horse = ModelBuilder::new("Horse")
            .nav_p("persons", "Person", NavigationPropertyKind::ManyToMany)
            .id_pk()
            .build();

        let new_model = json!({
            "id": 1,
            "horses": [
                {
                    "id": 1,
                },
                {
                    "id": 2,
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
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 7);

        let stmt1 = &res[0];
        expected_str!(stmt1.query, r#"INSERT INTO "Person" ("id") VALUES (?)"#);
        assert_eq!(*stmt1.values, vec![1]);

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"INSERT INTO "Horse" ("id") VALUES (?)"#);
        assert_eq!(*stmt2.values, vec![1]);

        let stmt3 = &res[2];
        expected_str!(
            stmt3.query,
            r#"INSERT INTO "HorsePerson" ("left", "right") VALUES (?, ?) ON CONFLICT  DO NOTHING"#
        );
        assert_eq!(*stmt3.values, vec![1, 1]);

        let stmt4 = &res[3];
        expected_str!(stmt4.query, r#"INSERT INTO "Horse" ("id") VALUES (?)"#);
        assert_eq!(*stmt4.values, vec![2]);

        let stmt5 = &res[4];
        expected_str!(
            stmt5.query,
            r#"INSERT INTO "HorsePerson" ("left", "right") VALUES (?, ?) ON CONFLICT  DO NOTHING"#
        );
        assert_eq!(*stmt5.values, vec![2, 1]);

        let stmt6 = &res[5];
        expected_str!(stmt6.query, r#"WHERE "id" = ?"#);
        assert_eq!(*stmt6.values, vec![1]);

        let results = test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let value = Value::from_str(results[5][0].try_get(0).unwrap()).unwrap();
        assert_eq!(value, Value::Array(vec![new_model]));
    }

    #[sqlx::test]
    async fn topological_ordering_is_correct(db: SqlitePool) {
        // Arrange
        let ast_person = ModelBuilder::new("Person")
            .id_pk()
            .col("horseId", CidlType::Integer, Some("Horse".into()))
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    column_reference: "horseId".to_string(),
                },
            )
            .build();

        let ast_horse = ModelBuilder::new("Horse")
            .id_pk()
            .nav_p(
                "awards",
                "Award",
                NavigationPropertyKind::OneToMany {
                    column_reference: "horseId".to_string(),
                },
            )
            .build();

        let ast_award = ModelBuilder::new("Award")
            .id_pk()
            .col("horseId", CidlType::Integer, Some("Horse".into()))
            .col("title", CidlType::Text, None)
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
            Some(include_tree.as_object().unwrap().clone()),
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
        let person = ModelBuilder::new("Person").id_pk().build();
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

        let stmt1 = &res[0];
        expected_str!(stmt1.query, r#"INSERT INTO "Person" DEFAULT VALUES"#);
        assert_eq!(stmt1.values.len(), 0);

        let stmt2 = &res[1];
        expected_str!(
            stmt2.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*stmt2.values, vec!["Person.id"]);

        let stmt3 = &res[2];
        expected_str!(
            stmt3.query,
            r#"SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?"#
        );
        assert_eq!(*stmt3.values, vec!["Person.id"]);

        let results = test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let value = Value::from_str(results[2][0].try_get(0).unwrap()).unwrap();
        assert_eq!(value, Value::Array(vec![json!({"id": 1})]));
    }

    #[sqlx::test]
    async fn insert_missing_one_to_one_fk_autogenerates(db: SqlitePool) {
        let person = ModelBuilder::new("Person")
            .id_pk()
            .col("horseId", CidlType::Integer, Some("Horse".into()))
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    column_reference: "horseId".into(),
                },
            )
            .build();

        let horse = ModelBuilder::new("Horse").id_pk().build();

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
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        assert_eq!(res.len(), 6);

        let stmt1 = &res[0];
        expected_str!(stmt1.query, r#"INSERT INTO "Horse" DEFAULT VALUES"#);
        assert_eq!(stmt1.values.len(), 0);

        let stmt2 = &res[1];
        expected_str!(
            stmt2.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*stmt2.values, vec!["Person.horse.id"]);

        let stmt3 = &res[2];
        expected_str!(
            stmt3.query,
            r#"INSERT INTO "Person" ("horseId") VALUES ((SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?))"#
        );
        assert_eq!(*stmt3.values, vec!["Person.horse.id"]);

        let stmt4 = &res[3];
        expected_str!(
            stmt4.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*stmt4.values, vec!["Person.id"]);

        let stmt5 = &res[4];
        expected_str!(
            stmt5.query,
            r#"SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?"#
        );
        assert_eq!(*stmt5.values, vec!["Person.id"]);

        let results = test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let value = Value::from_str(results[4][0].try_get(0).unwrap()).unwrap();
        assert_eq!(
            value,
            Value::Array(vec![json!({"id": 1, "horseId": 1, "horse": {"id": 1}})])
        );
    }

    #[sqlx::test]
    async fn insert_missing_one_to_many_fk_autogenerates(db: SqlitePool) {
        // Arrange
        let person = ModelBuilder::new("Person")
            .id_pk()
            .nav_p(
                "horses",
                "Horse",
                NavigationPropertyKind::OneToMany {
                    column_reference: "personId".into(),
                },
            )
            .build();

        let horse = ModelBuilder::new("Horse")
            .id_pk()
            .col("personId", CidlType::Integer, Some("Person".into()))
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
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert

        let stmt1 = &res[0];
        expected_str!(stmt1.query, r#"INSERT INTO "Person" DEFAULT VALUES"#);
        assert_eq!(stmt1.values.len(), 0);

        let stmt2 = &res[1];
        expected_str!(
            stmt2.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*stmt2.values, vec!["Person.id"]);

        let stmt3 = &res[2];
        expected_str!(
            stmt3.query,
            r#"INSERT INTO "Horse" ("personId") VALUES ((SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?))"#
        );
        assert_eq!(*stmt3.values, vec!["Person.id"]);

        let stmt4 = &res[3];
        expected_str!(
            stmt4.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*stmt4.values, vec!["Person.horses.id"]);

        let stmt5 = &res[4];
        expected_str!(
            stmt5.query,
            r#"SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?"#
        );
        assert_eq!(*stmt5.values, vec!["Person.id"]);

        let results = test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let value = Value::from_str(results[4][0].try_get(0).unwrap()).unwrap();
        assert_eq!(
            value,
            Value::Array(vec![
                json!({"id": 1, "horses": [ { "id": 1, "personId": 1 } ]})
            ])
        );
    }

    #[sqlx::test]
    async fn insert_missing_many_to_many_pk_fk_autogenerates(db: SqlitePool) {
        // Arrange
        let person = ModelBuilder::new("Person")
            .id_pk()
            .nav_p("horses", "Horse", NavigationPropertyKind::ManyToMany)
            .build();

        let horse = ModelBuilder::new("Horse")
            .nav_p("persons", "Person", NavigationPropertyKind::ManyToMany)
            .id_pk()
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
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert

        let stmt1 = &res[0];
        expected_str!(stmt1.query, r#"INSERT INTO "Person" DEFAULT VALUES"#);
        assert_eq!(stmt1.values.len(), 0);

        let stmt2 = &res[1];
        expected_str!(
            stmt2.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*stmt2.values, vec!["Person.id"]);

        let stmt3 = &res[2];
        expected_str!(stmt3.query, r#"INSERT INTO "Horse" DEFAULT VALUES"#);
        assert_eq!(stmt3.values.len(), 0);

        let stmt4 = &res[3];
        expected_str!(
            stmt4.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*stmt4.values, vec!["Person.horses.id"]);

        let stmt5 = &res[4];
        expected_str!(
            stmt5.query,
            r#"INSERT INTO "HorsePerson" ("left", "right") VALUES ((SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?), (SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?)) ON CONFLICT  DO NOTHING"#
        );
        assert_eq!(*stmt5.values, vec!["Person.horses.id", "Person.id"]);

        let stmt6 = &res[5];
        expected_str!(stmt6.query, r#"INSERT INTO "Horse" DEFAULT VALUES"#);
        assert_eq!(stmt6.values.len(), 0);

        let stmt7 = &res[6];
        expected_str!(
            stmt7.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "id") VALUES (?, last_insert_rowid())"#
        );
        assert_eq!(*stmt7.values, vec!["Person.horses.id"]);

        let stmt8 = &res[7];
        expected_str!(
            stmt8.query,
            r#"INSERT INTO "HorsePerson" ("left", "right") VALUES ((SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?), (SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?)) ON CONFLICT  DO NOTHING"#
        );
        assert_eq!(*stmt8.values, vec!["Person.horses.id", "Person.id"]);

        let stmt9 = &res[8];
        expected_str!(
            stmt9.query,
            r#"SELECT "id" FROM "_cloesce_tmp" WHERE "path" = ?"#
        );
        assert_eq!(*stmt9.values, vec!["Person.id"]);

        let results = test_sql(
            meta,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let value = Value::from_str(results[8][0].try_get(0).unwrap()).unwrap();
        assert_eq!(
            value,
            Value::Array(vec![json!({
                "id": 1,
                "horses": [
                    { "id": 1 },
                    { "id": 2 }
                ]
            })])
        );
    }
}
