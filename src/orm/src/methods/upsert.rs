use std::collections::HashMap;

use ast::{
    CidlType, CloesceAst, Model, NamedTypedValue, NavigationProperty, NavigationPropertyKind, fail,
};

use sea_query::{Alias, OnConflict, SimpleExpr, SqliteQueryBuilder, SubQueryStatement};
use sea_query::{Expr, Query};
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

use crate::IncludeTreeJson;
use crate::methods::select::SelectModel;
use crate::methods::{OrmErrorKind, alias};

use super::Result;
use super::validate::validate_cidl_type;

#[derive(Serialize)]
pub struct SqlStatement {
    pub query: String,
    pub values: Vec<serde_json::Value>,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct KvUpload {
    pub namespace_binding: String,
    pub key: String,
    pub value: Value,
    pub metadata: Value,
}

#[derive(Serialize, Debug, PartialEq)]
pub struct DelayedKvUpload {
    pub path: Vec<String>,
    pub namespace_binding: String,
    pub key: String,
    pub value: Value,
    pub metadata: Value,
}

#[derive(Serialize)]
pub struct UpsertResult {
    pub sql: Vec<SqlStatement>,
    kv_uploads: Vec<KvUpload>,
    kv_delayed_uploads: Vec<DelayedKvUpload>,
}

pub struct UpsertModel<'a> {
    ast: &'a CloesceAst,
    context: HashMap<String, Option<Value>>,
    sql_acc: Vec<SqlStatement>,
    kv_upload_acc: Vec<KvUpload>,
    kv_delayed_upload_acc: Vec<DelayedKvUpload>,
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
        model_name: &'a str,
        ast: &'a CloesceAst,
        new_model: Map<String, Value>,
        include_tree: Option<IncludeTreeJson>,
    ) -> Result<UpsertResult> {
        let include_tree = include_tree.unwrap_or_default();

        let mut generator = Self {
            ast,
            context: HashMap::default(),
            sql_acc: Vec::default(),
            kv_upload_acc: Vec::default(),
            kv_delayed_upload_acc: Vec::default(),
        };
        generator.dfs(
            None,
            model_name,
            new_model,
            &include_tree,
            model_name.to_string(),
        )?;

        // unwrap: root model is guaranteed to exist if we've gotten this far
        let model = ast.models.get(model_name).unwrap();
        if let Some(pk_col) = &model.primary_key {
            // Final select to return the upserted model
            let select_query = SelectModel::query(model_name, None, Some(include_tree), ast)?
                .trim_start_matches("SELECT ")
                .to_string();

            let mut select = Query::select();
            let select_root_model = select
                .expr(Expr::cust(&select_query))
                .and_where(
                    Expr::col((alias(&model.name), alias(&pk_col.name))).eq(
                        match generator
                            .context
                            .get(&format!("{}.{}", model.name, pk_col.name))
                        {
                            Some(Some(value)) => validate_and_transform(
                                model.primary_key.as_ref().unwrap().cidl_type.clone(),
                                value.clone(),
                                ast,
                            )?,
                            _ => SqlUpsertBuilder::value_from_ctx(&format!(
                                "{}.{}",
                                model.name, pk_col.name
                            )),
                        },
                    ),
                )
                .to_owned();

            generator.sql_acc.push(build_sqlite(select_root_model));
            generator.sql_acc.push(SqlStatement {
                query: VariablesTable::delete_all(),
                values: vec![],
            });
        }

        Ok(UpsertResult {
            sql: generator.sql_acc,
            kv_uploads: generator.kv_upload_acc,
            kv_delayed_uploads: generator.kv_delayed_upload_acc,
        })
    }

    // post order traversal for sql dependencies
    fn dfs(
        &mut self,
        parent_model_name: Option<&String>,
        model_name: &str,
        mut new_model: Map<String, Value>,
        include_tree: &IncludeTreeJson,
        path: String,
    ) -> Result<()> {
        let model = match self.ast.models.get(model_name) {
            Some(m) => m,
            None => fail!(OrmErrorKind::UnknownModel, "{}", model_name),
        };

        // KV objects
        for kv in &model.kv_objects {
            // TODO: Lists?
            let Some(Value::Object(mut kv_object)) = new_model.remove(&kv.value.name) else {
                fail!(
                    OrmErrorKind::TypeMismatch,
                    "{}.{} must be an object",
                    model.name,
                    kv.value.name
                )
            };

            let Some(value) = kv_object.remove("raw") else {
                fail!(
                    OrmErrorKind::MissingAttribute,
                    "{}.{} missing 'raw' field",
                    model.name,
                    kv.value.name
                )
            };
            let metadata = kv_object.remove("metadata").unwrap_or(Value::Null);

            let (key, placeholders_remain) =
                key_format_interpolation(&kv.format, &new_model, model)?;

            if placeholders_remain {
                let path_parts: Vec<String> = path.split('.').skip(1).map(String::from).collect();
                self.kv_delayed_upload_acc.push(DelayedKvUpload {
                    path: path_parts,
                    namespace_binding: kv.namespace_binding.clone(),
                    key,
                    value,
                    metadata,
                })
            } else {
                self.kv_upload_acc.push(KvUpload {
                    namespace_binding: kv.namespace_binding.clone(),
                    key,
                    value,
                    metadata,
                })
            }
        }

        let Some(pk_meta) = &model.primary_key else {
            return Ok(());
        };

        let mut builder = SqlUpsertBuilder::new(
            model_name,
            model.columns.len(),
            model.primary_key.as_ref().unwrap(),
            self.ast,
        );

        // Primary key
        let pk_val = match new_model.remove(&pk_meta.name) {
            Some(val) => Some(val),
            None if matches!(
                model.primary_key.as_ref().unwrap().cidl_type,
                CidlType::Integer
            ) =>
            {
                // Assume auto-generated
                None
            }
            _ => {
                fail!(
                    OrmErrorKind::MissingPrimaryKey,
                    "{}.{}",
                    model.name,
                    serde_json::to_string(&new_model).unwrap()
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
            let Some(Value::Object(nav_model)) = new_model.remove(&nav.var_name) else {
                continue;
            };
            let NavigationPropertyKind::OneToOne { column_reference } = &nav.kind else {
                continue;
            };

            // Recursively handle nested inserts
            self.dfs(
                Some(&model.name),
                &nav.model_reference,
                nav_model,
                nested_tree,
                format!("{path}.{}", nav.var_name),
            )?;

            let nav_model_pk = self
                .ast
                .models
                .get(&nav.model_reference)
                .expect("nav model to exist")
                .primary_key
                .as_ref()
                .expect("nav model to have a primary key");
            let ctx_path = format!("{path}.{}.{}", nav.var_name, nav_model_pk.name);
            nav_ref_to_path.insert(column_reference, ctx_path);
        }

        // Scalar attributes; attempt to retrieve FK's by value or context
        {
            // If this model is depends on another, it's dependency will have been inserted
            // before this model. Thus, it's parent pk has been inserted into the context under this path:
            let parent_id_path = parent_model_name.map(|p| {
                format!(
                    "{}.{}",
                    path.rsplit_once('.').map(|(h, _)| h).unwrap_or(&path),
                    self.ast
                        .models
                        .get(p)
                        .unwrap()
                        .primary_key
                        .as_ref()
                        .unwrap()
                        .name
                )
            });

            for attr in &model.columns {
                let path_key = nav_ref_to_path
                    .get(&attr.value.name)
                    .or(parent_id_path.as_ref());

                match (
                    new_model.remove(&attr.value.name),
                    &attr.foreign_key_reference,
                ) {
                    (Some(value), _) => {
                        // A value was provided in `new_model`
                        builder.push_val(&attr.value.name, value, &attr.value.cidl_type)?;
                    }
                    (None, Some(_)) if path_key.is_some() => {
                        let path_key = path_key.unwrap();
                        let ctx = self.context.get(path_key).expect("Context path missing");
                        builder.push_val_ctx(
                            ctx,
                            &attr.value.name,
                            &attr.value.cidl_type,
                            path_key,
                        )?;
                    }
                    (None, None) if pk_val.is_some() => {
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

        // All sql dependencies have been resolved by this point.
        self.upsert_table(pk_val, &path, pk_meta, builder)?;

        // Traverse navigation properties, using the include tree as a guide
        for nav in others {
            let Some(Value::Object(nested_tree)) = include_tree.get(&nav.var_name) else {
                continue;
            };

            match (&nav.kind, new_model.remove(&nav.var_name)) {
                (NavigationPropertyKind::OneToMany { .. }, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models {
                        let Value::Object(obj) = nav_model else {
                            continue;
                        };

                        self.dfs(
                            Some(&model.name),
                            &nav.model_reference,
                            obj,
                            nested_tree,
                            format!("{path}.{}", nav.var_name),
                        )?;
                    }
                }
                (NavigationPropertyKind::ManyToMany, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models {
                        let Value::Object(obj) = nav_model else {
                            continue;
                        };

                        self.dfs(
                            Some(&model.name),
                            &nav.model_reference,
                            obj,
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

        Ok(())
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
        let nav_meta = self.ast.models.get(&nav.model_reference).unwrap();
        let nav_pk = nav_meta.primary_key.as_ref().unwrap();
        let model_pk = model.primary_key.as_ref().unwrap();

        // Resolve both sides of the M:M relationship
        let mut pairs = [
            (
                nav.model_reference.as_str(),
                &nav_pk.cidl_type,
                format!("{path}.{}.{}", nav.var_name, nav_pk.name),
            ),
            (
                model.name.as_str(),
                &model_pk.cidl_type,
                format!("{path}.{}", model_pk.name),
            ),
        ];
        pairs.sort_by(|a, b| a.0.cmp(b.0));

        // Collect column/value pairs from context
        let mut entries = Vec::new();
        for (i, (_, cidl_type, path_key)) in pairs.iter().enumerate() {
            let value = match self.context.get(path_key).cloned().flatten() {
                Some(v) => validate_and_transform(cidl_type.to_owned().clone(), v, self.ast)?,
                None => SqlUpsertBuilder::value_from_ctx(path_key),
            };

            let col_name = if i == 0 { "left" } else { "right" };
            entries.push((col_name.to_string(), value));
        }

        // Build INSERT
        let mut insert = Query::insert();
        insert
            .into_table(alias(unique_id))
            .on_conflict(OnConflict::new().do_nothing().to_owned())
            .columns(entries.iter().map(|(col, _)| alias(col)))
            .values_panic(entries.into_iter().map(|(_, val)| val));

        self.sql_acc.push(build_sqlite(insert));
        Ok(())
    }

    /// Inserts the [InsertBuilder], updating the graph context to include the tables id.
    ///
    /// Returns an error if foreign key values exist that can not be resolved.
    fn upsert_table(
        &mut self,
        pk_val: Option<Value>,
        path: &str,
        primary_key: &NamedTypedValue,
        builder: SqlUpsertBuilder,
    ) -> Result<()> {
        // Add this models primary key to the context so dependents can resolve it
        let id_path = format!("{path}.{}", primary_key.name);
        self.sql_acc.push(builder.build(&pk_val)?);

        match pk_val {
            None => {
                self.sql_acc.push(VariablesTable::insert_rowid(&id_path));
                self.context.insert(id_path.clone(), None);
            }
            Some(val) => {
                self.context.insert(id_path.clone(), Some(val));
            }
        }

        Ok(())
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

    fn insert_rowid(path: &str) -> SqlStatement {
        build_sqlite(
            Query::insert()
                .into_table(alias(VARIABLES_TABLE_NAME))
                .columns(vec![alias("path"), alias("id")])
                .values_panic(vec![
                    Expr::val(path).into(),
                    Expr::cust("last_insert_rowid()"),
                ])
                .replace()
                .to_owned(),
        )
    }
}

struct SqlUpsertBuilder<'a> {
    model_name: &'a str,
    scalar_len: usize,
    cols: Vec<Alias>,
    vals: Vec<SimpleExpr>,
    pk_ntv: &'a NamedTypedValue,
    ast: &'a CloesceAst,
}

impl<'a> SqlUpsertBuilder<'a> {
    fn new(
        model_name: &'a str,
        scalar_len: usize,
        pk_ntv: &'a NamedTypedValue,
        ast: &'a CloesceAst,
    ) -> SqlUpsertBuilder<'a> {
        Self {
            scalar_len,
            model_name,
            pk_ntv,
            cols: Vec::default(),
            vals: Vec::default(),
            ast,
        }
    }

    /// Adds a column and value to the insert statement.
    ///
    /// TODO: Copies the value
    ///
    /// Returns an error if the value does not match the meta type.
    fn push_val(&mut self, var_name: &str, value: Value, cidl_type: &CidlType) -> Result<()> {
        self.cols.push(alias(var_name));
        let val = validate_and_transform(cidl_type.clone(), value, self.ast)?;
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
                self.push_val(var_name, v.clone(), cidl_type)?;
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
    fn build(self, pk_val: &Option<Value>) -> Result<SqlStatement> {
        let pk_expr = pk_val
            .as_ref()
            .map(|v| validate_and_transform(self.pk_ntv.cidl_type.clone(), v.clone(), self.ast))
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

            return Ok(build_sqlite(update));
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
        if pk_val.is_some() && !self.cols.is_empty() {
            insert.on_conflict(
                OnConflict::column(alias(&self.pk_ntv.name))
                    .update_columns(self.cols)
                    .to_owned(),
            );
        }

        Ok(build_sqlite(insert))
    }
}

/// Validates that each parameter in a key format (a string with `{placeholders}`)
/// exists in the new model as a stringifiable value.
///
/// Primary keys can be missing and will be left in the key format for later resolution.
///
/// Returns None if any required parameter is missing, otherwise returns the formatted key
/// and if any placeholders remain.
fn key_format_interpolation(
    key_format: &str,
    new_model: &Map<String, Value>,
    meta: &Model,
) -> Result<(String, bool)> {
    let mut key = key_format.to_string();
    let mut placeholders_remain = false;

    for cap in key_format.match_indices('{') {
        let end_brace = match key_format[cap.0..].find('}') {
            Some(idx) => cap.0 + idx,
            None => panic!("Unclosed brace in key format: {}", key_format),
        };
        let param_name = &key_format[cap.0 + 1..end_brace];
        let param_value = match new_model.get(param_name) {
            Some(v) => v,
            None => {
                if let Some(pk) = &meta.primary_key
                    && pk.name == param_name
                {
                    placeholders_remain = true;
                    continue;
                }

                fail!(
                    OrmErrorKind::MissingKeyParameter,
                    "{}.{} requires parameter '{}'",
                    meta.name,
                    key_format,
                    param_name
                )
            }
        };

        let replacement = match param_value {
            Value::String(s) => s.clone(),
            Value::Number(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            _ => fail!(
                OrmErrorKind::TypeMismatch,
                "{}.{} parameter '{}' must be string, number, or boolean",
                meta.name,
                key_format,
                param_name
            ),
        };

        key = key.replace(&format!("{{{}}}", param_name), &replacement);
    }

    Ok((key, placeholders_remain))
}

/// Convert a SeaQuery value to a serde_json value
fn sea_query_to_serde(v: sea_query::Value) -> serde_json::Value {
    match v {
        sea_query::Value::Int(Some(i)) => Value::from(i),
        sea_query::Value::Int(None) => Value::Null,
        sea_query::Value::BigInt(Some(i)) => Value::from(i),
        sea_query::Value::BigInt(None) => Value::Null,
        sea_query::Value::String(Some(s)) => Value::String(*s),
        sea_query::Value::String(None) => Value::Null,
        sea_query::Value::Float(Some(f)) => Value::from(f),
        sea_query::Value::Double(Some(d)) => Value::from(d),
        _ => unimplemented!("Value type not implemented in upsert serde conversion"),
    }
}

fn build_sqlite<T: sea_query::QueryStatementWriter>(qb: T) -> SqlStatement {
    let (query, vs) = qb.build(SqliteQueryBuilder);
    SqlStatement {
        query,
        values: vs
            .into_iter()
            .map(sea_query_to_serde)
            .collect::<Vec<serde_json::Value>>(),
    }
}

fn validate_and_transform(
    cidl_type: CidlType,
    value: Value,
    ast: &CloesceAst,
) -> Result<SimpleExpr> {
    let res = validate_cidl_type(cidl_type, Some(value), ast, false);
    let value = match res {
        Ok(Some(v)) => v,
        Ok(None) => fail!(
            OrmErrorKind::MissingAttribute,
            "An attribute is missing a value"
        ),
        Err(e) => fail!(
            OrmErrorKind::TypeMismatch,
            "Value does not match expected type: {:?}",
            e
        ),
    };

    Ok(match value {
        Value::Null => SimpleExpr::Custom("null".into()),
        Value::Bool(b) => Expr::val(if b { 1 } else { 0 }).into(),
        Value::Number(n) if n.is_i64() => Expr::val(n.as_i64().unwrap()).into(),
        Value::Number(n) if n.is_f64() => Expr::val(n.as_f64().unwrap()).into(),
        Value::String(s) => Expr::val(s).into(),

        // Must be a u8 array, so convert to hex string for SQLite
        Value::Array(arr) => {
            let hex = arr
                .iter()
                .map(|b| format!("{:02X}", b.as_i64().unwrap()))
                .collect::<String>();
            SimpleExpr::Custom(format!("X'{}'", hex))
        }
        _ => unreachable!("validate_cidl_type should have caught this"),
    })
}

#[cfg(test)]
mod test {
    use ast::{CidlType, NavigationPropertyKind};
    use generator_test::{ModelBuilder, create_ast, expected_str};
    use serde_json::{Value, json};
    use sqlx::{Row, SqlitePool};

    use crate::methods::{
        OrmErrorKind, test_sql,
        upsert::{DelayedKvUpload, KvUpload, UpsertModel, key_format_interpolation},
    };

    #[test]
    fn test_key_format_interpolation() {
        // Substitutes
        {
            // Arrange
            let key_format = "User/{id}/{foo}/{bar}";
            let new_model = json!({
                "id": 1,
                "foo": "hello",
                "bar": false
            });

            // Act
            let res = key_format_interpolation(
                key_format,
                new_model.as_object().unwrap(),
                &ModelBuilder::new("User").id_pk().build(),
            );

            // Assert
            assert_eq!(res.unwrap(), ("User/1/hello/false".to_string(), false));
        }

        // Returns placeholder on missing PK
        {
            // Arrange
            let model = ModelBuilder::new("User").id_pk().build();
            let key_format = "User/{id}/";
            let new_model = json!({});

            // Act
            let res = key_format_interpolation(key_format, new_model.as_object().unwrap(), &model);

            // Assert
            assert_eq!(res.unwrap(), (key_format.to_string(), true));
        }

        // Returns OrmError on missing required param
        {
            // Arrange
            let model = ModelBuilder::new("User").id_pk().build();
            let key_format = "User/{id}/{foo}/";
            let new_model = json!({
                "id": 1
            });

            // Act
            let res = key_format_interpolation(key_format, new_model.as_object().unwrap(), &model);

            // Assert
            assert!(res.is_err());
            assert!(matches!(
                res.err().unwrap().kind,
                OrmErrorKind::MissingKeyParameter
            ));
        }
    }

    #[sqlx::test]
    async fn upsert_scalar_model(db: SqlitePool) {
        // Arrange
        let model = ModelBuilder::new("Horse")
            .id_pk()
            .col("color", CidlType::Text, None)
            .col("age", CidlType::Integer, None)
            .col("address", CidlType::nullable(CidlType::Text), None)
            .col("is_tired", CidlType::Boolean, None)
            .build();

        let new_model = json!({
            "id": 1,
            "color": "brown",
            "age": 7,
            "address": null,
            "is_tired": true
        });

        let ast = create_ast(vec![model]);

        // Act
        let res = UpsertModel::query("Horse", &ast, new_model.as_object().unwrap().clone(), None)
            .unwrap()
            .sql;

        // Assert
        assert_eq!(res.len(), 3);

        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "Horse" ("color", "age", "address",  "is_tired", "id") VALUES (?, ?, null, ?, ?)"#
        );
        expected_str!(
            stmt1.query,
            r#"ON CONFLICT ("id") DO UPDATE SET "color" = "excluded"."color", "age" = "excluded"."age", "address" = "excluded"."address", "is_tired" = "excluded"."is_tired"#
        );
        assert_eq!(
            *stmt1.values,
            vec![
                Value::from("brown"),
                Value::from(7i64),
                Value::from(1i64),
                Value::from(1i64)
            ]
        );

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"WHERE "Horse"."id" = ?"#);
        assert_eq!(*stmt2.values, vec![Value::from(1i64)]);

        let stmt3 = &res[2];
        expected_str!(stmt3.query, r#"DELETE FROM "_cloesce_tmp""#);
        assert_eq!(stmt3.values.len(), 0);

        let results = test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let row = &results[1][0];
        assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
        assert_eq!(row.try_get::<String, _>("color").unwrap(), "brown");
        assert_eq!(row.try_get::<i64, _>("age").unwrap(), 7);
        assert_eq!(row.try_get::<Option<String>, _>("address").unwrap(), None);
        assert!(row.try_get::<bool, _>("is_tired").unwrap());
    }

    #[sqlx::test]
    async fn update_scalar_model(db: SqlitePool) {
        // Arrange
        let model = ModelBuilder::new("Horse")
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

        let ast = create_ast(vec![model]);

        // Act
        let res = UpsertModel::query("Horse", &ast, new_model.as_object().unwrap().clone(), None)
            .unwrap()
            .sql;

        // Assert
        assert_eq!(res.len(), 3);

        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"UPDATE "Horse" SET "age" = ?, "address" = null WHERE "id" = ?"#
        );
        assert_eq!(*stmt1.values, vec![Value::from(7), Value::from(1)]);

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"WHERE "Horse"."id" = ?"#);
        assert_eq!(*stmt2.values, vec![Value::from(1i64)]);

        test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn upsert_blob_b64(db: SqlitePool) {
        // Arrange
        let model = ModelBuilder::new("Picture")
            .id_pk()
            .col("metadata", CidlType::Text, None)
            .col("blob", CidlType::Blob, None)
            .build();

        let ast = create_ast(vec![model]);

        let new_model = json!({
            "id": 1,
            "metadata": "meta",
            "blob": "aGVsbG8gd29ybGQ="
        });

        // Act
        let res = UpsertModel::query(
            "Picture",
            &ast,
            new_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap()
        .sql;

        // Assert
        assert_eq!(res.len(), 3);

        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "Picture" ("metadata", "blob", "id") VALUES (?, X'68656C6C6F20776F726C64', ?) ON CONFLICT ("id") DO UPDATE SET "metadata" = "excluded"."metadata", "blob" = "excluded"."blob""#
        );
        assert_eq!(*stmt1.values, vec![Value::from("meta"), Value::from(1i64),]);

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"WHERE "Picture"."id" = ?"#);
        assert_eq!(*stmt2.values, vec![Value::from(1i64)]);

        let results = test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let row = &results[1][0];
        assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
        assert_eq!(row.try_get::<String, _>("metadata").unwrap(), "meta");
        let blob: Vec<u8> = row.try_get::<Vec<u8>, _>("blob").unwrap();
        assert_eq!(blob, b"hello world");
    }

    #[sqlx::test]
    async fn upsert_blob_u8_arr(db: SqlitePool) {
        // Arrange
        let model = ModelBuilder::new("Picture")
            .id_pk()
            .col("metadata", CidlType::Text, None)
            .col("blob", CidlType::Blob, None)
            .build();

        let ast = create_ast(vec![model]);

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
            &ast,
            new_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap()
        .sql;

        // Assert
        assert_eq!(res.len(), 3);

        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "Picture" ("metadata", "blob", "id") VALUES (?, X'68656C6C6F20776F726C64', ?) ON CONFLICT ("id") DO UPDATE SET "metadata" = "excluded"."metadata", "blob" = "excluded"."blob""#
        );
        assert_eq!(*stmt1.values, vec![Value::from("meta"), Value::from(1i64),]);

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"WHERE "Picture"."id" = ?"#);
        assert_eq!(*stmt2.values, vec![Value::from(1i64)]);

        test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn one_to_one(db: SqlitePool) {
        // Arrange
        let person = ModelBuilder::new("Person")
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
        let horse = ModelBuilder::new("Horse").id_pk().build();

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

        let ast = create_ast(vec![person, horse]);

        // Act
        let res = UpsertModel::query(
            "Person",
            &ast,
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap()
        .sql;

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
        expected_str!(stmt3.query, r#"WHERE "Person"."id" = ?"#);
        assert_eq!(*stmt3.values, vec![1]);

        let results = test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let row = &results[2][0];
        assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("horseId").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("horse.id").unwrap(), 1);
    }

    #[sqlx::test]
    async fn one_to_many(db: SqlitePool) {
        // Arrange
        let person = ModelBuilder::new("Person")
            .id_pk()
            .nav_p(
                "horses",
                "Horse",
                NavigationPropertyKind::OneToMany {
                    column_reference: "personId".to_string(),
                },
            )
            .build();
        let horse = ModelBuilder::new("Horse")
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

        let ast = create_ast(vec![person, horse]);

        // Act
        let res = UpsertModel::query(
            "Person",
            &ast,
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap()
        .sql;

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
        expected_str!(stmt5.query, r#"WHERE "Person"."id" = ?"#);
        assert_eq!(*stmt5.values, vec![1]);

        let results = test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let row = &results[4][0];
        assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("horses.id").unwrap(), 1);
    }

    #[sqlx::test]
    async fn many_to_many(db: SqlitePool) {
        // Arrange
        let person = ModelBuilder::new("Person")
            .id_pk()
            .nav_p("horses", "Horse", NavigationPropertyKind::ManyToMany)
            .build();
        let horse = ModelBuilder::new("Horse")
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

        let ast = create_ast(vec![person, horse]);

        // Act
        let res = UpsertModel::query(
            "Person",
            &ast,
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap()
        .sql;

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
        expected_str!(stmt6.query, r#"WHERE "Person"."id" = ?"#);
        assert_eq!(*stmt6.values, vec![1]);

        let results = test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let row = &results[5][0];
        assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("horses.id").unwrap(), 1);
    }

    #[sqlx::test]
    async fn topological_ordering_is_correct(db: SqlitePool) {
        // Arrange
        let person = ModelBuilder::new("Person")
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

        let horse = ModelBuilder::new("Horse")
            .id_pk()
            .nav_p(
                "awards",
                "Award",
                NavigationPropertyKind::OneToMany {
                    column_reference: "horseId".to_string(),
                },
            )
            .build();

        let award = ModelBuilder::new("Award")
            .id_pk()
            .col("horseId", CidlType::Integer, Some("Horse".into()))
            .col("title", CidlType::Text, None)
            .build();

        let ast = create_ast(vec![person, horse, award]);

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
            &ast,
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap()
        .sql;

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
            ast,
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
        let ast = create_ast(vec![person]);

        let new_person = json!({});

        // Act
        let res = UpsertModel::query(
            "Person",
            &ast,
            new_person.as_object().unwrap().clone(),
            None,
        )
        .unwrap()
        .sql;

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
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let row = &results[2][0];
        assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
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

        let ast = create_ast(vec![person, horse]);

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
            &ast,
            new_person.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap()
        .sql;

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
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let row = &results[4][0];
        assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("horseId").unwrap(), 1);
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

        let ast = create_ast(vec![person, horse]);

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
            &ast,
            new_person.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap()
        .sql;

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
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let row = &results[4][0];
        assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("horses.id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("horses.personId").unwrap(), 1);
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

        let ast = create_ast(vec![person, horse]);

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
            &ast,
            new_person.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap()
        .sql;

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
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");

        let row = &results[8][0];
        assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("horses.id").unwrap(), 1);
    }

    #[sqlx::test]
    async fn kv_objects(db: SqlitePool) {
        // Arrange
        let person = ModelBuilder::new("Person")
            .id_pk()
            .col("name", CidlType::Text, None)
            .col("horseId", CidlType::Integer, Some("Horse".into()))
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    column_reference: "horseId".to_string(),
                },
            )
            // requires primary key to be set
            .kv_object(
                "person/{id}/profile",
                "PERSON_KV",
                "profile",
                false,
                CidlType::Text,
            )
            .build();

        let horse = ModelBuilder::new("Horse")
            .id_pk()
            .nav_p(
                "awards",
                "Award",
                NavigationPropertyKind::OneToMany {
                    column_reference: "horseId".to_string(),
                },
            )
            // requires primary key to be set
            .kv_object(
                "horse/{id}/stats",
                "HORSE_KV",
                "stats",
                false,
                CidlType::Text,
            )
            .build();

        let award = ModelBuilder::new("Award")
            .id_pk()
            .col("horseId", CidlType::Integer, Some("Horse".into()))
            .col("title", CidlType::Text, None)
            // requires primary key to be set
            .kv_object(
                "award/{id}/certificate",
                "AWARD_KV",
                "certificate",
                false,
                CidlType::Text,
            )
            .build();

        let ast = create_ast(vec![person, horse, award]);

        let new_model = json!({
            "id": 100,
            "name": "Alice",
            "profile": {
                "raw": {"bio": "Horse trainer"},
                "metadata": {"version": 1}
            },
            "horse": {
                // id is not set

                "stats": {
                    "raw": {"wins": 10, "losses": 2},
                    "metadata": null
                },
                "awards": [
                    {
                        "id": 500,
                        "title": "Best in Show",
                        "certificate": {
                            "raw": {"issuer": "Racing Association"},
                            "metadata": {"year": 2024}
                        }
                    },
                    {
                        "id": 501,
                        "title": "Speed Champion",
                        "certificate": {
                            "raw": {"issuer": "Speed League"},
                            "metadata": {"year": 2024}
                        }
                    }
                ]
            }
        });

        let include_tree = json!({
            "horse": {
                "awards": {}
            }
        });

        // Act
        let result = UpsertModel::query(
            "Person",
            &ast,
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap();

        // Assert
        assert_eq!(
            result.kv_uploads,
            vec![
                KvUpload {
                    namespace_binding: "PERSON_KV".to_string(),
                    key: "person/100/profile".to_string(),
                    value: json!({"bio": "Horse trainer"}),
                    metadata: json!({"version": 1}),
                },
                KvUpload {
                    namespace_binding: "AWARD_KV".to_string(),
                    key: "award/500/certificate".to_string(),
                    value: json!({"issuer": "Racing Association"}),
                    metadata: json!({"year": 2024}),
                },
                KvUpload {
                    namespace_binding: "AWARD_KV".to_string(),
                    key: "award/501/certificate".to_string(),
                    value: json!({"issuer": "Speed League"}),
                    metadata: json!({"year": 2024}),
                },
            ]
        );

        assert_eq!(
            result.kv_delayed_uploads,
            vec![DelayedKvUpload {
                path: vec!["horse".to_string()],
                namespace_binding: "HORSE_KV".to_string(),
                key: "horse/{id}/stats".to_string(),
                value: json!({"wins": 10, "losses": 2}),
                metadata: Value::Null,
            }]
        );

        test_sql(
            ast,
            result
                .sql
                .into_iter()
                .map(|r| (r.query, r.values))
                .collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[test]
    fn pure_kv_object() {
        // Arrange
        // (an object that has only KV properties, no table columns or pk)
        let model = ModelBuilder::new("Config")
            .key_param("key")
            .kv_object("config/{key}", "CONFIG_KV", "data", false, CidlType::Text)
            .build();

        let ast = create_ast(vec![model]);
        let new_model = json!({
            "key": "site-settings",
            "data": {
                "raw": {"theme": "dark", "itemsPerPage": 20},
                "metadata": {"version": 3}
            }
        });

        // Act
        let result =
            UpsertModel::query("Config", &ast, new_model.as_object().unwrap().clone(), None)
                .unwrap();

        // Assert
        assert_eq!(
            result.kv_uploads,
            vec![KvUpload {
                namespace_binding: "CONFIG_KV".to_string(),
                key: "config/site-settings".to_string(),
                value: json!({"theme": "dark", "itemsPerPage": 20}),
                metadata: json!({"version": 3}),
            },]
        );
    }
}
