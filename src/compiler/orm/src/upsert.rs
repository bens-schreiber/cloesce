use std::collections::HashMap;

use ast::{
    CidlType, CloesceAst, Column, IncludeTree, Model, NavigationField, NavigationFieldKind,
    ValidatedField,
};

use sea_query::{Alias, OnConflict, SimpleExpr, SqliteQueryBuilder};
use sea_query::{Expr, Query};
use serde::Serialize;
use serde_json::Map;
use serde_json::Value;

use crate::fail;
use crate::select::SelectModel;
use crate::{OrmErrorKind, alias};

use super::Result;
use super::validate::validate_cidl_type;

type IncludeTreeJson = Map<String, Value>;

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
    ast: &'a CloesceAst<'a>,
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
        ast: &'a CloesceAst<'a>,
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

        let model = ast.models.get(model_name).expect("Model to exist");
        if model.has_d1() {
            // Final select to return the upserted model
            let include_tree_json_str = serde_json::to_string(&include_tree).unwrap_or_default();
            let include_tree_typed: IncludeTree =
                serde_json::from_str(&include_tree_json_str).unwrap_or_default();
            let select_query = SelectModel::query(model_name, None, Some(include_tree_typed), ast)?
                .trim_start_matches("SELECT ")
                .to_string();

            let mut select = Query::select();
            let mut select_root_model = select.expr(Expr::cust(&select_query));

            // Add WHERE clause for each primary key column
            // e.g., WHERE "Model"."id" = (SELECT json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Model')
            for col in &model.primary_columns {
                let pk_path = format!("{}.{}", model.name, col.field.name);
                let pk_expr = match generator.context.get(&pk_path) {
                    Some(Some(value)) => validate_and_transform(&col.field, value, ast)?,
                    _ => SqlUpsertBuilder::value_from_ctx(&pk_path),
                };
                select_root_model = select_root_model.and_where(
                    Expr::col((alias(model.name), alias(col.field.name.as_ref()))).eq(pk_expr),
                );
            }

            generator
                .sql_acc
                .push(build_sqlite(select_root_model.to_owned()));
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
        parent_model_name: Option<&str>,
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
        for kv in &model.kv_fields {
            // TODO: Lists?
            let Some(Value::Object(mut kv_object)) = new_model.remove(kv.field.name.as_ref())
            else {
                fail!(
                    OrmErrorKind::TypeMismatch,
                    "{}.{} must be an object",
                    model.name,
                    kv.field.name
                )
            };

            let Some(value) = kv_object.remove("raw") else {
                fail!(
                    OrmErrorKind::MissingAttribute,
                    "{}.{} missing 'raw' field",
                    model.name,
                    kv.field.name
                )
            };
            let metadata = kv_object.remove("metadata").unwrap_or(Value::Null);

            let (key, placeholders_remain) =
                key_format_interpolation(kv.format, &new_model, model)?;

            if placeholders_remain {
                let path_parts: Vec<String> = path.split('.').skip(1).map(String::from).collect();
                self.kv_delayed_upload_acc.push(DelayedKvUpload {
                    path: path_parts,
                    namespace_binding: kv.binding.to_string(),
                    key,
                    value,
                    metadata,
                })
            } else {
                self.kv_upload_acc.push(KvUpload {
                    namespace_binding: kv.binding.to_string(),
                    key,
                    value,
                    metadata,
                })
            }
        }

        if !model.has_d1() {
            return Ok(());
        }

        let mut builder = SqlUpsertBuilder::new(model_name, &model.primary_columns, self.ast);

        // Primary keys
        let mut pk_vals: Vec<(String, Option<Value>)> = Vec::new();
        let mut pk_missing = true;
        for pk_col in &model.primary_columns {
            match new_model.remove(pk_col.field.name.as_ref()) {
                Some(val) => {
                    pk_vals.push((pk_col.field.name.to_string(), Some(val)));
                    pk_missing = false;
                }
                None if matches!(pk_col.field.cidl_type, CidlType::Int | CidlType::Uint) => {
                    if model.has_composite_pk() {
                        fail!(
                            OrmErrorKind::CompositeKeyCannotAutoincrement,
                            "{}: composite keys cannot be auto-incremented, all key columns must be provided",
                            model.name
                        );
                    }

                    // The value is auto incremented and will be generated on insertion.
                    pk_vals.push((pk_col.field.name.to_string(), None));
                }
                _ => {
                    fail!(
                        OrmErrorKind::MissingPrimaryKey,
                        "{}.{}",
                        model.name,
                        serde_json::to_string(&new_model).unwrap()
                    );
                }
            }
        }

        let (one_to_ones, others): (Vec<_>, Vec<_>) = model
            .navigation_fields
            .iter()
            .partition(|n| matches!(n.kind, NavigationFieldKind::OneToOne { .. }));

        // This table is dependent on it's 1:1 references, so they must be traversed before
        // table insertion (granted the include tree references them).
        let mut nav_ref_to_path = HashMap::new();
        for nav in one_to_ones {
            let Some(Value::Object(nested_tree)) = include_tree.get(nav.field.name.as_ref()) else {
                continue;
            };
            let Some(Value::Object(nav_model)) = new_model.remove(nav.field.name.as_ref()) else {
                continue;
            };
            let NavigationFieldKind::OneToOne {
                columns: key_columns,
            } = &nav.kind
            else {
                continue;
            };

            // Recursively handle nested inserts
            self.dfs(
                Some(model.name),
                nav.model_reference,
                nav_model,
                nested_tree,
                format!("{path}.{}", nav.field.name),
            )?;

            // Map each key column to the path in the context wheere its value can be found
            for key in key_columns {
                let (col, _) = model
                    .all_columns()
                    .find(|(c, _)| c.field.name == *key)
                    .expect("key column to exist in model");

                let fk = col
                    .foreign_key_reference
                    .as_ref()
                    .expect("foreign key to exist");

                nav_ref_to_path.insert(
                    *key,
                    format!("{path}.{}.{}", nav.field.name, fk.column_name),
                );
            }
        }

        // Scalar attributes; attempt to retrieve FK's by value or context
        {
            // If this model depends on another, its dependency will have been inserted
            // before this model. Thus, its parent pks exist in the context and can be used for FK resolution.
            let parent_id_paths: Vec<String> = parent_model_name
                .map(|p| {
                    let parent_path = path.rsplit_once('.').map(|(h, _)| h).unwrap_or(&path);
                    self.ast
                        .models
                        .get(p)
                        .expect("Parent model not found in AST")
                        .primary_columns
                        .iter()
                        .map(|pk_col| format!("{}.{}", parent_path, pk_col.field.name))
                        .collect()
                })
                .unwrap_or_default();

            for attr in &model.columns {
                let path_key = nav_ref_to_path.get(attr.field.name.as_ref()).or_else(|| {
                    // Check if this column is part of a foreign key from parent
                    let fk = &attr.foreign_key_reference.as_ref()?;
                    let parent_name = parent_model_name?;

                    if fk.model_name != parent_name {
                        return None;
                    }

                    // Find matching parent pk path
                    parent_id_paths
                        .iter()
                        .find(|p| p.ends_with(&fk.column_name))
                });

                match (
                    new_model.remove(attr.field.name.as_ref()),
                    &attr.foreign_key_reference,
                ) {
                    (Some(value), _) => {
                        // A value was provided in `new_model`
                        builder.push_val(&attr.field, &value)?;
                    }
                    (None, Some(_)) if path_key.is_some() => {
                        // No value provided, but this column is a FK reference
                        // that can be resolved through context
                        let path_key = path_key.unwrap();
                        let ctx = self.context.get(path_key).expect("Context path missing");
                        builder.push_val_ctx(&attr.field, ctx, path_key)?;
                    }
                    (None, _) if pk_missing && attr.field.cidl_type.is_nullable() => {
                        // Default to null for INSERT (no PK provided).
                        builder.push_val(&attr.field, &Value::Null)?;
                    }
                    (None, _) if !pk_missing => {
                        if !attr.field.cidl_type.is_nullable() {
                            // A non-nullable column is missing and we have a PK. This forces an UPDATE.
                            builder.flag_missing_non_nullable();
                        }
                    }
                    _ => {
                        fail!(
                            OrmErrorKind::MissingAttribute,
                            "{}.{}: {}",
                            model.name,
                            attr.field.name,
                            serde_json::to_string(&new_model).unwrap()
                        );
                    }
                };
            }
        }

        // All sql dependencies have been resolved by this point.
        self.upsert_table(&pk_vals, &path, builder)?;

        // Traverse navigation properties, using the include tree as a guide
        for nav in others {
            let Some(Value::Object(nested_tree)) = include_tree.get(nav.field.name.as_ref()) else {
                continue;
            };

            match (&nav.kind, new_model.remove(nav.field.name.as_ref())) {
                (NavigationFieldKind::OneToMany { .. }, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models {
                        let Value::Object(obj) = nav_model else {
                            continue;
                        };

                        self.dfs(
                            Some(model.name),
                            nav.model_reference,
                            obj,
                            nested_tree,
                            format!("{path}.{}", nav.field.name),
                        )?;
                    }
                }
                (NavigationFieldKind::ManyToMany, Some(Value::Array(nav_models))) => {
                    for nav_model in nav_models {
                        let Value::Object(obj) = nav_model else {
                            continue;
                        };

                        self.dfs(
                            Some(model.name),
                            nav.model_reference,
                            obj,
                            nested_tree,
                            format!("{path}.{}", nav.field.name),
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
        nav: &NavigationField,
        unique_id: &str,
        model: &Model,
    ) -> Result<()> {
        let nav_meta = self.ast.models.get(&nav.model_reference).unwrap();

        // Resolve both sides of the M:M relationship
        // Each side may have multiple PK columns (composite keys)
        let mut left_entries = Vec::new();
        let mut right_entries = Vec::new();

        // Collect nav model PKs
        for pk_col in &nav_meta.primary_columns {
            let path_key = format!("{path}.{}.{}", nav.field.name, pk_col.field.name);
            let value = match self.context.get(&path_key).and_then(|v| v.as_ref()) {
                Some(v) => validate_and_transform(&pk_col.field, v, self.ast)?,
                None => SqlUpsertBuilder::value_from_ctx(&path_key),
            };
            left_entries.push(value);
        }

        // Collect current models PKs
        for pk_col in &model.primary_columns {
            let path_key = format!("{path}.{}", pk_col.field.name);
            let value = match self.context.get(&path_key).and_then(|v| v.as_ref()) {
                Some(v) => validate_and_transform(&pk_col.field, v, self.ast)?,
                None => SqlUpsertBuilder::value_from_ctx(&path_key),
            };
            right_entries.push(value);
        }

        // Determine which is "left" and which is "right" (alphabetically)
        let (left_vals, right_vals, left_model, right_model) = if nav.model_reference < model.name {
            (left_entries, right_entries, nav_meta, model)
        } else {
            (right_entries, left_entries, model, nav_meta)
        };

        let mut columns = Vec::new();
        let mut values = Vec::new();

        // Left side columns
        for (pk_col, val) in left_model.primary_columns.iter().zip(left_vals) {
            let col_name = if left_model.primary_columns.len() == 1 {
                "left".to_string()
            } else {
                format!("left_{}", pk_col.field.name)
            };
            columns.push(alias(&col_name));
            values.push(val);
        }

        // Right side columns
        for (pk_col, val) in right_model.primary_columns.iter().zip(right_vals) {
            let col_name = if right_model.primary_columns.len() == 1 {
                "right".to_string()
            } else {
                format!("right_{}", pk_col.field.name)
            };
            columns.push(alias(&col_name));
            values.push(val);
        }

        // Build INSERT
        let mut insert = Query::insert();
        insert
            .into_table(alias(unique_id))
            .on_conflict(OnConflict::new().do_nothing().to_owned())
            .columns(columns)
            .values_panic(values);

        self.sql_acc.push(build_sqlite(insert));
        Ok(())
    }

    /// Inserts the [SqlUpsertBuilder], updating the graph context to include the tables id.
    ///
    /// Returns an error if foreign key values exist that can not be resolved.
    fn upsert_table(
        &mut self,
        pk_vals: &[(String, Option<Value>)],
        path: &str,
        builder: SqlUpsertBuilder,
    ) -> Result<()> {
        self.sql_acc.push(builder.build(pk_vals)?);

        // Add primary keys to the context
        let all_pk_provided = pk_vals.iter().all(|(_, v)| v.is_some());

        if all_pk_provided {
            // All PKs provided, store them in context
            for (pk_name, pk_val) in pk_vals {
                let id_path = format!("{path}.{}", pk_name);
                self.context.insert(id_path.clone(), pk_val.clone());
            }

            return Ok(());
        }

        if pk_vals.len() != 1 {
            unreachable!("Only single column PKs can be auto-generated")
        }

        // The PK is not composite and is auto generated, we can retrieve the last
        // inserted rowid and store it in context.
        // The path for the JSON object is just the model path (not per-column)
        self.sql_acc.push(VariablesTable::insert_pk(path, pk_vals));

        // Mark PK column in context as needing lookup
        let id_path = format!("{path}.{}", pk_vals[0].0);
        self.context.insert(id_path.clone(), None);

        Ok(())
    }
}

const VARIABLES_TABLE_NAME: &str = "_cloesce_tmp";
const VARIABLES_TABLE_COL_PATH: &str = "path";
const VARIABLES_TABLE_COL_PRIMARY_KEY: &str = "primary_key";

/// A cloesce-shipped table that for storing temporary SQL
/// values, needed for complex insertions.
///
/// Always stores primary keys as JSON objects for consistency,
/// e.g., {"id": 1} for single PK or {"orderId": 1, "productId": 2} for composite.
///
/// Unfortunately, D1 supports only read-only CTE's, so this temp table is
/// the only option available to us.
///
/// See https://github.com/bens-schreiber/cloesce/blob/schreiber/orm-ctes/src/runtime/src/methods/insert.rs
/// for a CTE based solution if that ever changes.
struct VariablesTable;
impl VariablesTable {
    fn delete_all() -> String {
        Query::delete()
            .from_table(alias(VARIABLES_TABLE_NAME))
            .to_string(SqliteQueryBuilder)
    }

    /// Insert primary key(s) as JSON into the variables table.
    fn insert_pk(path: &str, pk_columns: &[(String, Option<Value>)]) -> SqlStatement {
        // Build JSON object with column names as keys
        let mut json_parts = Vec::new();
        for (col_name, val_opt) in pk_columns {
            let val_expr = match val_opt {
                Some(Value::Number(n)) if n.is_i64() => n.to_string(),
                Some(Value::String(s)) => format!("'{}'", s.replace("'", "''")),
                None => "last_insert_rowid()".to_string(),
                _ => "last_insert_rowid()".to_string(),
            };
            json_parts.push(format!("'{}', {}", col_name, val_expr));
        }

        let json_expr = format!("json_object({})", json_parts.join(", "));

        build_sqlite(
            Query::insert()
                .into_table(alias(VARIABLES_TABLE_NAME))
                .columns(vec![alias("path"), alias("primary_key")])
                .values_panic(vec![Expr::val(path).into(), Expr::cust(&json_expr)])
                .replace()
                .to_owned(),
        )
    }
}

struct SqlUpsertBuilder<'a> {
    model_name: &'a str,
    cols: Vec<Alias>,
    vals: Vec<SimpleExpr>,
    pk_cols: &'a [Column<'a>],
    ast: &'a CloesceAst<'a>,
    has_missing_non_nullable: bool,
}

impl<'a> SqlUpsertBuilder<'a> {
    fn new(
        model_name: &'a str,
        pk_cols: &'a [Column<'a>],
        ast: &'a CloesceAst<'a>,
    ) -> SqlUpsertBuilder<'a> {
        Self {
            model_name,
            pk_cols,
            cols: Vec::default(),
            vals: Vec::default(),
            ast,
            has_missing_non_nullable: false,
        }
    }

    /// Adds a column and value to the insert statement.
    ///
    /// Returns an error if the value does not match the meta type.
    fn push_val(&mut self, field: &ValidatedField, value: &Value) -> Result<()> {
        self.cols.push(alias(field.name.as_ref()));
        let val = validate_and_transform(field, value, self.ast)?;
        self.vals.push(val);
        Ok(())
    }

    /// Adds a column and value from the graph context.
    fn push_val_ctx(
        &mut self,
        field: &ValidatedField,
        ctx: &Option<Value>,
        path: &str,
    ) -> Result<()> {
        match ctx {
            None => {
                self.cols.push(alias(field.name.as_ref()));
                self.vals.push(Self::value_from_ctx(path));
            }
            Some(v) => {
                self.push_val(field, v)?;
            }
        }
        Ok(())
    }

    /// Force an UPDATE instead of an INSERT
    fn flag_missing_non_nullable(&mut self) {
        self.has_missing_non_nullable = true;
    }

    /// Generates a subquery expression to retrieve a value from the context based on the path.
    fn value_from_ctx(path: &str) -> SimpleExpr {
        // "Model.columnName" => "columnName"
        let col_name = path.rsplit('.').next().unwrap_or(path);

        // Subquery to retrieve the value from the variables table based on the path.
        let base_path = &path[..path.rfind('.').unwrap_or(path.len())];
        let json_extract = format!(
            "(SELECT json_extract({}, '$.{}') FROM {} WHERE {} = '{}')",
            VARIABLES_TABLE_COL_PRIMARY_KEY,
            col_name,
            VARIABLES_TABLE_NAME,
            VARIABLES_TABLE_COL_PATH,
            base_path
        );

        SimpleExpr::Custom(json_extract)
    }

    /// Creates a SQL query, being either an update, insert, or upsert.
    fn build(self, pk_vals: &[(String, Option<Value>)]) -> Result<SqlStatement> {
        let all_pk_provided = pk_vals.iter().all(|(_, v)| v.is_some());
        let any_pk_provided = pk_vals.iter().any(|(_, v)| v.is_some());

        // Build expressions for each PK
        let mut pk_exprs: Vec<(String, SimpleExpr)> = Vec::new();
        for (pk_col, (pk_name, pk_val)) in self.pk_cols.iter().zip(pk_vals.iter()) {
            let expr = match pk_val {
                Some(v) => validate_and_transform(&pk_col.field, v, self.ast)?,
                None => {
                    // Value will come from context (auto-generated)
                    continue;
                }
            };
            pk_exprs.push((pk_name.clone(), expr));
        }

        // If we have PKs and a non-nullable column is missing, this must be an UPDATE
        if any_pk_provided && self.has_missing_non_nullable {
            let mut update = Query::update();
            let mut update_stmt = update
                .table(alias(self.model_name))
                .values(self.cols.into_iter().zip(self.vals));

            // Add WHERE clause for each PK
            for (pk_name, pk_expr) in pk_exprs {
                update_stmt = update_stmt.and_where(Expr::col(alias(&pk_name)).eq(pk_expr));
            }

            return Ok(build_sqlite(update_stmt.to_owned()));
        }

        // Build an INSERT
        let mut insert = Query::insert();
        insert.into_table(alias(self.model_name));

        let mut cols = self.cols.clone();
        let mut vals = self.vals.clone();

        // Add provided PKs to insert
        for (pk_name, pk_expr) in &pk_exprs {
            cols.push(alias(pk_name));
            vals.push(pk_expr.clone());
        }

        insert.columns(cols.clone()).values_panic(vals);
        if cols.is_empty() {
            insert.or_default_values();
        }

        // If we have all PKs and some attributes, enable conflict handling for upsert
        if all_pk_provided && !self.cols.is_empty() {
            // Build the ON CONFLICT clause with all PK columns
            let pk_column_names: Vec<Alias> =
                pk_exprs.iter().map(|(name, _)| alias(name)).collect();
            insert.on_conflict(
                OnConflict::columns(pk_column_names)
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
fn key_format_interpolation(
    key_format: &str,
    new_model: &Map<String, Value>,
    meta: &Model,
) -> Result<(String, bool)> {
    let mut placeholders_remain = false;

    let mut result = String::with_capacity(key_format.len());
    let mut last_end = 0;
    for (start, _) in key_format.match_indices('{') {
        result.push_str(&key_format[last_end..start]);
        let end = match key_format[start..].find('}') {
            Some(idx) => start + idx,
            None => unreachable!("Unclosed brace in key format: {}", key_format),
        };
        let param_name = &key_format[start + 1..end];
        let param_value = match new_model.get(param_name) {
            Some(v) => v,
            None => {
                if meta
                    .all_columns()
                    .any(|(col, _)| col.field.name == param_name)
                {
                    placeholders_remain = true;
                    result.push_str(&format!("{{{}}}", param_name));
                    last_end = end + 1;
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

        result.push_str(&replacement);
        last_end = end + 1;
    }

    result.push_str(&key_format[last_end..]);

    Ok((result, placeholders_remain))
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
        sea_query::Value::Float(None) => Value::Null,
        sea_query::Value::Double(Some(d)) => Value::from(d),
        sea_query::Value::Double(None) => Value::Null,
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
    field: &ValidatedField,
    value: &Value,
    ast: &CloesceAst,
) -> Result<SimpleExpr> {
    let res = validate_cidl_type(
        field.cidl_type.clone(),
        &field.validators,
        Some(value.clone()),
        ast,
        false,
    );
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
        Value::Null => match field.cidl_type.root_type() {
            CidlType::Int => return Ok(Expr::val(None::<i32>).into()),
            CidlType::Uint => return Ok(Expr::val(None::<u32>).into()),
            CidlType::Boolean => return Ok(Expr::val(None::<bool>).into()),
            CidlType::Real => return Ok(Expr::val(None::<f64>).into()),
            CidlType::String | CidlType::DateIso => {
                return Ok(Expr::val(None::<String>).into());
            }
            CidlType::Blob => return Ok(Expr::val(None::<Vec<u8>>).into()),
            _ => unreachable!("Invalid CIDL"),
        },
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
