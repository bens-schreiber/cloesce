use std::collections::HashMap;

use ast::{
    CidlType, CloesceAst, D1Column, Model, NavigationProperty, NavigationPropertyKind, fail,
};

use sea_query::{Alias, OnConflict, SimpleExpr, SqliteQueryBuilder};
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
        if model.has_d1() {
            // Final select to return the upserted model
            let select_query = SelectModel::query(model_name, None, Some(include_tree), ast)?
                .trim_start_matches("SELECT ")
                .to_string();

            let mut select = Query::select();
            let mut select_root_model = select.expr(Expr::cust(&select_query));

            // Add WHERE clause for each primary key column
            // e.g., WHERE "Model"."id" = (SELECT json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Model')
            for pk_col in &model.primary_key_columns {
                let pk_path = format!("{}.{}", model.name, pk_col.value.name);
                let pk_expr = match generator.context.get(&pk_path) {
                    Some(Some(value)) => {
                        validate_and_transform(pk_col.value.cidl_type.clone(), value, ast)?
                    }
                    _ => SqlUpsertBuilder::value_from_ctx(&pk_path),
                };
                select_root_model = select_root_model.and_where(
                    Expr::col((alias(&model.name), alias(&pk_col.value.name))).eq(pk_expr),
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

        if !model.has_d1() {
            return Ok(());
        }

        let mut builder = SqlUpsertBuilder::new(model_name, &model.primary_key_columns, self.ast);

        // Primary keys
        let mut pk_vals: Vec<(String, Option<Value>)> = Vec::new();
        let mut pk_missing = true;
        for pk_col in &model.primary_key_columns {
            match new_model.remove(&pk_col.value.name) {
                Some(val) => {
                    pk_vals.push((pk_col.value.name.clone(), Some(val)));
                    pk_missing = false;
                }
                None if matches!(pk_col.value.cidl_type, CidlType::Integer) => {
                    if model.has_composite_pk() {
                        fail!(
                            OrmErrorKind::CompositeKeyCannotAutoincrement,
                            "{}: composite keys cannot be auto-incremented, all key columns must be provided",
                            model.name
                        );
                    }

                    // The value is auto incremented and will be generated on insertion.
                    pk_vals.push((pk_col.value.name.clone(), None));
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
            let NavigationPropertyKind::OneToOne { key_columns } = &nav.kind else {
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

            // Map each key column to the path in the context wheere its value can be found
            for key in key_columns {
                let (col, _) = model
                    .all_columns()
                    .find(|(c, _)| c.value.name == *key)
                    .expect("key column to exist in model");

                let fk = col
                    .foreign_key_reference
                    .as_ref()
                    .expect("foreign key to exist");

                nav_ref_to_path.insert(
                    key.as_str(),
                    format!("{path}.{}.{}", nav.var_name, fk.column_name),
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
                        .primary_key_columns
                        .iter()
                        .map(|pk_col| format!("{}.{}", parent_path, pk_col.value.name))
                        .collect()
                })
                .unwrap_or_default();

            for attr in &model.columns {
                let path_key = nav_ref_to_path.get(attr.value.name.as_str()).or_else(|| {
                    // Check if this column is part of a foreign key from parent
                    let fk = &attr.foreign_key_reference.as_ref()?;
                    let parent_name = parent_model_name?;

                    if fk.model_name.as_str() != parent_name {
                        return None;
                    }

                    // Find matching parent pk path
                    parent_id_paths
                        .iter()
                        .find(|p| p.ends_with(&fk.column_name))
                });

                match (
                    new_model.remove(&attr.value.name),
                    &attr.foreign_key_reference,
                ) {
                    (Some(value), _) => {
                        // A value was provided in `new_model`
                        builder.push_val(&attr.value.name, &value, &attr.value.cidl_type)?;
                    }
                    (None, Some(_)) if path_key.is_some() => {
                        // No value provided, but this column is a FK reference
                        // that can be resolved through context
                        let path_key = path_key.unwrap();
                        let ctx = self.context.get(path_key).expect("Context path missing");
                        builder.push_val_ctx(
                            ctx,
                            &attr.value.name,
                            &attr.value.cidl_type,
                            path_key,
                        )?;
                    }
                    (None, _) if pk_missing && attr.value.cidl_type.is_nullable() => {
                        // Default to null for INSERT (no PK provided).
                        builder.push_val(&attr.value.name, &Value::Null, &attr.value.cidl_type)?;
                    }
                    (None, _) if !pk_missing => {
                        if !attr.value.cidl_type.is_nullable() {
                            // A non-nullable column is missing and we have a PK. This forces an UPDATE.
                            builder.flag_missing_non_nullable();
                        }
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
        self.upsert_table(&pk_vals, &path, builder)?;

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

        // Resolve both sides of the M:M relationship
        // Each side may have multiple PK columns (composite keys)
        let mut left_entries = Vec::new();
        let mut right_entries = Vec::new();

        // Collect nav model PKs
        for pk_col in &nav_meta.primary_key_columns {
            let path_key = format!("{path}.{}.{}", nav.var_name, pk_col.value.name);
            let value = match self.context.get(&path_key).and_then(|v| v.as_ref()) {
                Some(v) => validate_and_transform(pk_col.value.cidl_type.clone(), v, self.ast)?,
                None => SqlUpsertBuilder::value_from_ctx(&path_key),
            };
            left_entries.push(value);
        }

        // Collect current models PKs
        for pk_col in &model.primary_key_columns {
            let path_key = format!("{path}.{}", pk_col.value.name);
            let value = match self.context.get(&path_key).and_then(|v| v.as_ref()) {
                Some(v) => validate_and_transform(pk_col.value.cidl_type.clone(), v, self.ast)?,
                None => SqlUpsertBuilder::value_from_ctx(&path_key),
            };
            right_entries.push(value);
        }

        // Determine which is "left" and which is "right" (alphabetically)
        let (left_vals, right_vals, left_model, right_model) =
            if nav.model_reference.as_str() < model.name.as_str() {
                (left_entries, right_entries, nav_meta, model)
            } else {
                (right_entries, left_entries, model, nav_meta)
            };

        let mut columns = Vec::new();
        let mut values = Vec::new();

        // Left side columns
        for (pk_col, val) in left_model
            .primary_key_columns
            .iter()
            .zip(left_vals.into_iter())
        {
            let col_name = if left_model.primary_key_columns.len() == 1 {
                "left".to_string()
            } else {
                format!("left_{}", pk_col.value.name)
            };
            columns.push(alias(&col_name));
            values.push(val);
        }

        // Right side columns
        for (pk_col, val) in right_model
            .primary_key_columns
            .iter()
            .zip(right_vals.into_iter())
        {
            let col_name = if right_model.primary_key_columns.len() == 1 {
                "right".to_string()
            } else {
                format!("right_{}", pk_col.value.name)
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
    pk_cols: &'a [D1Column],
    ast: &'a CloesceAst,
    has_missing_non_nullable: bool,
}

impl<'a> SqlUpsertBuilder<'a> {
    fn new(
        model_name: &'a str,
        pk_cols: &'a [D1Column],
        ast: &'a CloesceAst,
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
    fn push_val(&mut self, var_name: &str, value: &Value, cidl_type: &CidlType) -> Result<()> {
        self.cols.push(alias(var_name));
        let val = validate_and_transform(cidl_type.clone(), value, self.ast)?;
        self.vals.push(val);
        Ok(())
    }

    /// Adds a column and value from the graph context.
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
                Some(v) => validate_and_transform(pk_col.value.cidl_type.clone(), v, self.ast)?,
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
///
/// Returns None if any required parameter is missing, otherwise returns the formatted key
/// and if any placeholders remain.
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
                    .any(|(col, _)| col.value.name == param_name)
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
    cidl_type: CidlType,
    value: &Value,
    ast: &CloesceAst,
) -> Result<SimpleExpr> {
    let res = validate_cidl_type(cidl_type.clone(), Some(value.clone()), ast, false);
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
        Value::Null => match cidl_type.root_type() {
            CidlType::Integer => return Ok(Expr::val(None::<i64>).into()),
            CidlType::Boolean => return Ok(Expr::val(None::<bool>).into()),
            CidlType::Real => return Ok(Expr::val(None::<f64>).into()),
            CidlType::Text | CidlType::DateIso => {
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

#[cfg(test)]
mod test {
    use std::{collections::HashMap, path::PathBuf};

    use ast::{CidlType, ForeignKeyReference, NavigationPropertyKind};
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
                &ModelBuilder::new("User").default_db().id_pk().build(),
            );

            // Assert
            assert_eq!(res.unwrap(), ("User/1/hello/false".to_string(), false));
        }

        // Returns placeholder on missing PK
        {
            // Arrange
            let model = ModelBuilder::new("User").default_db().id_pk().build();
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
            let model = ModelBuilder::new("User").default_db().id_pk().build();
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
            .default_db()
            .id_pk()
            .col("color", CidlType::Text, None, None)
            .col("age", CidlType::Integer, None, None)
            .col("address", CidlType::nullable(CidlType::Text), None, None)
            .col("is_tired", CidlType::Boolean, None, None)
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
            r#"INSERT INTO "Horse" ("color", "age", "address",  "is_tired", "id") VALUES (?, ?, ?, ?, ?)"#
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
                Value::from(None::<String>),
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
            .default_db()
            .id_pk()
            .col("color", CidlType::Text, None, None)
            .col("age", CidlType::Integer, None, None)
            .col("address", CidlType::nullable(CidlType::Text), None, None)
            .build();

        let new_model = json!({
            // pk exists
            "id": 1,
            "age": 7,
            "address": null

            // color is missing, so this should be an update.
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
            r#"UPDATE "Horse" SET "age" = ?, "address" = ? WHERE "id" = ?"#
        );
        assert_eq!(
            *stmt1.values,
            vec![
                Value::from(7),
                Value::from(None::<String>),
                Value::from(1i64)
            ]
        );

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
    async fn upsert_with_undefined_nullable_col(db: SqlitePool) {
        // Arrange
        let model = ModelBuilder::new("User")
            .default_db()
            .id_pk()
            .col("name", CidlType::Text, None, None)
            .col("age", CidlType::Integer, None, None)
            .col("nickname", CidlType::nullable(CidlType::Text), None, None)
            .build();

        let ast = create_ast(vec![model]);

        let new_model = json!({
            "id": 1,
            "name": "Bob",
            "age": 30,

            // nickname is nullable but is missing from the input
        });

        // Act
        let res = UpsertModel::query("User", &ast, new_model.as_object().unwrap().clone(), None)
            .unwrap()
            .sql;

        // Assert
        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "User" ("name", "age", "id") VALUES (?, ?, ?) ON CONFLICT ("id") DO UPDATE SET "name" = "excluded"."name", "age" = "excluded"."age""#
        );
        assert_eq!(
            *stmt1.values,
            vec![Value::from("Bob"), Value::from(30), Value::from(1i64)]
        );

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"WHERE "User"."id" = ?"#);
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
            .default_db()
            .id_pk()
            .col("metadata", CidlType::Text, None, None)
            .col("blob", CidlType::Blob, None, None)
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
            .default_db()
            .id_pk()
            .col("metadata", CidlType::Text, None, None)
            .col("blob", CidlType::Blob, None, None)
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
            .default_db()
            .id_pk()
            .col(
                "horseId",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Horse".into(),
                    column_name: "id".into(),
                }),
                None,
            )
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    key_columns: vec!["horseId".to_string()],
                },
            )
            .build();
        let horse = ModelBuilder::new("Horse").default_db().id_pk().build();

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
            .default_db()
            .id_pk()
            .nav_p(
                "horses",
                "Horse",
                NavigationPropertyKind::OneToMany {
                    key_columns: vec!["personId".to_string()],
                },
            )
            .build();
        let horse = ModelBuilder::new("Horse")
            .default_db()
            .id_pk()
            .col(
                "personId",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Person".into(),
                    column_name: "id".into(),
                }),
                None,
            )
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
            .default_db()
            .id_pk()
            .nav_p("horses", "Horse", NavigationPropertyKind::ManyToMany)
            .build();
        let horse = ModelBuilder::new("Horse")
            .default_db()
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
            .default_db()
            .id_pk()
            .col(
                "horseId",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Horse".into(),
                    column_name: "id".into(),
                }),
                None,
            )
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    key_columns: vec!["horseId".to_string()],
                },
            )
            .build();

        let horse = ModelBuilder::new("Horse")
            .default_db()
            .id_pk()
            .nav_p(
                "awards",
                "Award",
                NavigationPropertyKind::OneToMany {
                    key_columns: vec!["horseId".to_string()],
                },
            )
            .build();

        let award = ModelBuilder::new("Award")
            .default_db()
            .id_pk()
            .col(
                "horseId",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Horse".into(),
                    column_name: "id".into(),
                }),
                None,
            )
            .col("title", CidlType::Text, None, None)
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
        let person = ModelBuilder::new("Person").default_db().id_pk().build();
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
            r#"REPLACE INTO "_cloesce_tmp" ("path", "primary_key") VALUES (?, json_object('id', last_insert_rowid()))"#
        );
        assert_eq!(*stmt2.values, vec!["Person"]);

        let stmt3 = &res[2];
        expected_str!(
            stmt3.query,
            r#"json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Person'"#
        );

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
    async fn insert_empty(db: SqlitePool) {
        // Arrange
        let model = ModelBuilder::new("User")
            .default_db()
            .id_pk()
            .col("nickname", CidlType::nullable(CidlType::Text), None, None)
            .build();

        let ast = create_ast(vec![model]);

        let new_model = json!({
            // completely empty
        });

        // Act
        let res =
            UpsertModel::query("User", &ast, new_model.as_object().unwrap().clone(), None).unwrap();

        // Assert
        let stmt1 = &res.sql[0];
        expected_str!(stmt1.query, r#"INSERT INTO "User" ("nickname") VALUES (?)"#);
        assert_eq!(*stmt1.values, vec![Value::Null]);

        test_sql(
            ast,
            res.sql.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Upsert to work");
    }

    #[sqlx::test]
    async fn insert_with_undefined_nullable(db: SqlitePool) {
        // Arrange
        let model = ModelBuilder::new("User")
            .default_db()
            .id_pk()
            .col("name", CidlType::Text, None, None)
            .col("age", CidlType::Integer, None, None)
            .col(
                "bestFriend",
                CidlType::nullable(CidlType::Integer),
                Some(ForeignKeyReference {
                    model_name: "User".into(),
                    column_name: "id".into(),
                }),
                None,
            )
            .build();

        let new_model = json!({
            "name": "Bob",
            "age": 30,
            // bestFriend is nullable but is missing from the input
        });

        let ast = create_ast(vec![model]);

        // Act
        let res = UpsertModel::query("User", &ast, new_model.as_object().unwrap().clone(), None)
            .unwrap()
            .sql;

        // Assert
        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "User" ("name", "age", "bestFriend") VALUES (?, ?, ?)"#
        );
        assert_eq!(
            *stmt1.values,
            vec![Value::from("Bob"), Value::from(30), Value::Null]
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
    async fn insert_missing_one_to_one_fk_autogenerates(db: SqlitePool) {
        let person = ModelBuilder::new("Person")
            .default_db()
            .id_pk()
            .col(
                "horseId",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Horse".into(),
                    column_name: "id".into(),
                }),
                None,
            )
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    key_columns: vec!["horseId".into()],
                },
            )
            .build();

        let horse = ModelBuilder::new("Horse").default_db().id_pk().build();

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
            r#"REPLACE INTO "_cloesce_tmp" ("path", "primary_key") VALUES (?, json_object('id', last_insert_rowid()))"#
        );
        assert_eq!(*stmt2.values, vec!["Person.horse"]);

        let stmt3 = &res[2];
        expected_str!(
            stmt3.query,
            r#"INSERT INTO "Person" ("horseId") VALUES ((SELECT json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Person.horse'))"#
        );

        let stmt4 = &res[3];
        expected_str!(
            stmt4.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "primary_key") VALUES (?, json_object('id', last_insert_rowid()))"#
        );
        assert_eq!(*stmt4.values, vec!["Person"]);

        let stmt5 = &res[4];
        expected_str!(
            stmt5.query,
            r#"json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Person'"#
        );

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
            .default_db()
            .id_pk()
            .nav_p(
                "horses",
                "Horse",
                NavigationPropertyKind::OneToMany {
                    key_columns: vec!["personId".into()],
                },
            )
            .build();

        let horse = ModelBuilder::new("Horse")
            .default_db()
            .id_pk()
            .col(
                "personId",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Person".into(),
                    column_name: "id".into(),
                }),
                None,
            )
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
            r#"REPLACE INTO "_cloesce_tmp" ("path", "primary_key") VALUES (?, json_object('id', last_insert_rowid()))"#
        );
        assert_eq!(*stmt2.values, vec!["Person"]);

        let stmt3 = &res[2];
        expected_str!(
            stmt3.query,
            r#"INSERT INTO "Horse" ("personId") VALUES ((SELECT json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Person'))"#
        );

        let stmt4 = &res[3];
        expected_str!(
            stmt4.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "primary_key") VALUES (?, json_object('id', last_insert_rowid()))"#
        );
        assert_eq!(*stmt4.values, vec!["Person.horses"]);

        let stmt5 = &res[4];
        expected_str!(
            stmt5.query,
            r#"json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Person'"#
        );

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
            .default_db()
            .id_pk()
            .nav_p("horses", "Horse", NavigationPropertyKind::ManyToMany)
            .build();

        let horse = ModelBuilder::new("Horse")
            .default_db()
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
            r#"REPLACE INTO "_cloesce_tmp" ("path", "primary_key") VALUES (?, json_object('id', last_insert_rowid()))"#
        );
        assert_eq!(*stmt2.values, vec!["Person"]);

        let stmt3 = &res[2];
        expected_str!(stmt3.query, r#"INSERT INTO "Horse" DEFAULT VALUES"#);
        assert_eq!(stmt3.values.len(), 0);

        let stmt4 = &res[3];
        expected_str!(
            stmt4.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "primary_key") VALUES (?, json_object('id', last_insert_rowid()))"#
        );
        assert_eq!(*stmt4.values, vec!["Person.horses"]);

        let stmt5 = &res[4];
        expected_str!(
            stmt5.query,
            r#"INSERT INTO "HorsePerson" ("left", "right") VALUES ((SELECT json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Person.horses'), (SELECT json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Person'))"#
        );

        let stmt6 = &res[5];
        expected_str!(stmt6.query, r#"INSERT INTO "Horse" DEFAULT VALUES"#);
        assert_eq!(stmt6.values.len(), 0);

        let stmt7 = &res[6];
        expected_str!(
            stmt7.query,
            r#"REPLACE INTO "_cloesce_tmp" ("path", "primary_key") VALUES (?, json_object('id', last_insert_rowid()))"#
        );
        assert_eq!(*stmt7.values, vec!["Person.horses"]);

        let stmt8 = &res[7];
        expected_str!(
            stmt8.query,
            r#"INSERT INTO "HorsePerson" ("left", "right") VALUES ((SELECT json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Person.horses'), (SELECT json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Person'))"#
        );

        let stmt9 = &res[8];
        expected_str!(
            stmt9.query,
            r#"json_extract(primary_key, '$.id') FROM _cloesce_tmp WHERE path = 'Person'"#
        );

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
    async fn composite_primary_key_upsert(db: SqlitePool) {
        // Arrange
        let order_item = ModelBuilder::new("OrderItem")
            .default_db()
            .pk("order_id", CidlType::Integer)
            .pk("product_id", CidlType::Integer) // => composite PK of (order_id, product_id)
            .col("quantity", CidlType::Integer, None, None)
            .col("price", CidlType::Real, None, None)
            .build();

        let ast = create_ast(vec![order_item]);

        // Act
        let new_model = json!({
            "order_id": 1,
            "product_id": 2,
            "quantity": 5,
            "price": 29.99
        });

        let res = UpsertModel::query(
            "OrderItem",
            &ast,
            new_model.as_object().unwrap().clone(),
            None,
        )
        .unwrap()
        .sql;

        // Assert
        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "OrderItem" ("quantity", "price", "order_id", "product_id") VALUES (?, ?, ?, ?)"#
        );
        expected_str!(
            stmt1.query,
            r#"ON CONFLICT ("order_id", "product_id") DO UPDATE SET "quantity" = "excluded"."quantity", "price" = "excluded"."price""#
        );

        let stmt2 = &res[1];
        expected_str!(stmt2.query, r#"WHERE "OrderItem"."order_id" = ?"#);
        expected_str!(stmt2.query, r#"AND "OrderItem"."product_id" = ?"#);

        let results = test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Composite PK upsert to work");

        let row = &results[1][0];
        assert_eq!(row.try_get::<i64, _>("order_id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("product_id").unwrap(), 2);
        assert_eq!(row.try_get::<i64, _>("quantity").unwrap(), 5);
    }

    #[sqlx::test]
    async fn composite_fk_one_to_one(db: SqlitePool) {
        // Arrange
        let student = ModelBuilder::new("Student")
            .default_db()
            .pk("school_id", CidlType::Integer)
            .pk("student_number", CidlType::Integer) // => composite PK of (school_id, student_number)
            .col("name", CidlType::Text, None, None)
            .build();

        let enrollment = ModelBuilder::new("Enrollment")
            .default_db()
            .id_pk()
            .col(
                "school_id",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Student".into(),
                    column_name: "school_id".into(),
                }),
                Some(0), // Same composite_id for the composite FK
            )
            .col(
                "student_number",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Student".into(),
                    column_name: "student_number".into(),
                }),
                Some(0), // Same composite_id for the composite FK
            )
            .col("course", CidlType::Text, None, None)
            .nav_p(
                "student",
                "Student",
                NavigationPropertyKind::OneToOne {
                    key_columns: vec!["school_id".to_string(), "student_number".to_string()],
                },
            )
            .build();

        let ast = create_ast(vec![student, enrollment]);

        let new_model = json!({
            "id": 1,
            "course": "Math 101",
            "student": {
                "school_id": 10,
                "student_number": 5001,
                "name": "Alice"
            }
        });

        let include_tree = json!({
            "student": {}
        });

        // Act
        let res = UpsertModel::query(
            "Enrollment",
            &ast,
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap()
        .sql;

        // Assert
        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "Student" ("name", "school_id", "student_number") VALUES (?, ?, ?)"#
        );

        let stmt2 = &res[1];
        expected_str!(
            stmt2.query,
            r#"INSERT INTO "Enrollment" ("school_id", "student_number", "course", "id") VALUES (?, ?, ?, ?)"#
        );

        let results = test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Composite FK 1:1 to work");

        let row = &results[2][0];
        assert_eq!(row.try_get::<i64, _>("id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("school_id").unwrap(), 10);
        assert_eq!(row.try_get::<i64, _>("student_number").unwrap(), 5001);
        assert_eq!(row.try_get::<i64, _>("student.school_id").unwrap(), 10);
        assert_eq!(
            row.try_get::<i64, _>("student.student_number").unwrap(),
            5001
        );
    }

    #[sqlx::test]
    async fn composite_fk_one_to_many(db: SqlitePool) {
        let order = ModelBuilder::new("Order")
            .default_db()
            .pk("region_id", CidlType::Integer)
            .pk("order_number", CidlType::Integer) // => composite PK of (region_id, order_number)
            .col("customer", CidlType::Text, None, None)
            .nav_p(
                "items",
                "OrderItem",
                NavigationPropertyKind::OneToMany {
                    key_columns: vec!["region_id".to_string(), "order_number".to_string()],
                },
            )
            .build();

        let order_item = ModelBuilder::new("OrderItem")
            .default_db()
            .id_pk()
            .col(
                "region_id",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Order".into(),
                    column_name: "region_id".into(),
                }),
                Some(0), // Same composite_id for the composite FK
            )
            .col(
                "order_number",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Order".into(),
                    column_name: "order_number".into(),
                }),
                Some(0), // Same composite_id for the composite FK
            )
            .col("product", CidlType::Text, None, None)
            .build();

        let ast = create_ast(vec![order, order_item]);

        let new_model = json!({
            "region_id": 1,
            "order_number": 100,
            "customer": "Bob",
            "items": [
                {
                    "id": 1,
                    "product": "Widget"
                },
                {
                    "id": 2,
                    "product": "Gadget"
                }
            ]
        });

        let include_tree = json!({
            "items": {}
        });

        // Act
        let res = UpsertModel::query(
            "Order",
            &ast,
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap()
        .sql;

        // Assert
        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "Order" ("customer", "region_id", "order_number") VALUES (?, ?, ?)"#
        );

        let stmt2 = &res[1];
        expected_str!(
            stmt2.query,
            r#"INSERT INTO "OrderItem" ("region_id", "order_number", "product", "id") VALUES (?, ?, ?, ?)"#
        );

        let results = test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Composite FK 1:M to work");

        let row = &results[3][0];
        assert_eq!(row.try_get::<i64, _>("region_id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("order_number").unwrap(), 100);
        assert_eq!(row.try_get::<i64, _>("items.id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("items.region_id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("items.order_number").unwrap(), 100);
    }

    #[sqlx::test]
    async fn composite_pk_many_to_many(db: SqlitePool) {
        // Arrange
        let teacher = ModelBuilder::new("Teacher")
            .default_db()
            .pk("school_id", CidlType::Integer)
            .pk("employee_id", CidlType::Integer) // => composite PK of (school_id, employee_id)
            .col("name", CidlType::Text, None, None)
            .nav_p("courses", "Course", NavigationPropertyKind::ManyToMany)
            .build();

        let course = ModelBuilder::new("Course")
            .default_db()
            .pk("department_id", CidlType::Integer)
            .pk("course_code", CidlType::Integer)
            .col("title", CidlType::Text, None, None)
            .nav_p("teachers", "Teacher", NavigationPropertyKind::ManyToMany)
            .build();

        let ast = create_ast(vec![teacher, course]);

        let new_model = json!({
            "school_id": 1,
            "employee_id": 123,
            "name": "Dr. Smith",
            "courses": [
                {
                    "department_id": 10,
                    "course_code": 101,
                    "title": "Intro to CS"
                }
            ]
        });

        let include_tree = json!({
            "courses": {}
        });

        // Act
        let res = UpsertModel::query(
            "Teacher",
            &ast,
            new_model.as_object().unwrap().clone(),
            Some(include_tree.as_object().unwrap().clone()),
        )
        .unwrap()
        .sql;

        // Assert
        let stmt1 = &res[0];
        expected_str!(
            stmt1.query,
            r#"INSERT INTO "Teacher" ("name", "school_id", "employee_id") VALUES (?, ?, ?)"#
        );

        let stmt2 = &res[1];
        expected_str!(
            stmt2.query,
            r#"INSERT INTO "Course" ("title", "department_id", "course_code") VALUES (?, ?, ?)"#
        );

        let stmt3 = &res[2];
        expected_str!(stmt3.query, r#"INSERT INTO "CourseTeacher""#);
        expected_str!(
            stmt3.query,
            r#"("left_department_id", "left_course_code", "right_school_id", "right_employee_id")"#
        );

        let results = test_sql(
            ast,
            res.into_iter().map(|r| (r.query, r.values)).collect(),
            db,
        )
        .await
        .expect("Composite PK M:M to work");

        let row = &results[3][0];
        assert_eq!(row.try_get::<i64, _>("school_id").unwrap(), 1);
        assert_eq!(row.try_get::<i64, _>("employee_id").unwrap(), 123);
        assert_eq!(row.try_get::<i64, _>("courses.department_id").unwrap(), 10);
        assert_eq!(row.try_get::<i64, _>("courses.course_code").unwrap(), 101);
    }

    #[test]
    fn composite_key_cannot_autoincrement() {
        // Arrange
        let order_item = ModelBuilder::new("OrderItem")
            .default_db()
            .pk("order_id", CidlType::Integer)
            .pk("product_id", CidlType::Integer)
            .col("quantity", CidlType::Integer, None, None)
            .build();

        let ast = create_ast(vec![order_item]);

        // Act
        let new_model = json!({
            "quantity": 5
            // Missing both order_id and product_id
        });

        let result = UpsertModel::query(
            "OrderItem",
            &ast,
            new_model.as_object().unwrap().clone(),
            None,
        );

        // Assert
        assert!(result.is_err());
        if let Err(err) = result {
            assert!(matches!(
                err.kind,
                OrmErrorKind::CompositeKeyCannotAutoincrement
            ));
            assert!(
                err.context
                    .contains("composite keys cannot be auto-incremented")
            );
        }
    }

    #[test]
    fn composite_key_with_partial_keys_fails() {
        // Arrange
        let order_item = ModelBuilder::new("OrderItem")
            .default_db()
            .pk("order_id", CidlType::Integer)
            .pk("product_id", CidlType::Integer)
            .col("quantity", CidlType::Integer, None, None)
            .build();

        let ast = create_ast(vec![order_item]);

        // Act
        let new_model = json!({
            "order_id": 1,
            // Missing product_id
            "quantity": 5
        });

        let result = UpsertModel::query(
            "OrderItem",
            &ast,
            new_model.as_object().unwrap().clone(),
            None,
        );

        // Assert
        assert!(result.is_err());
        if let Err(err) = result {
            assert!(matches!(
                err.kind,
                OrmErrorKind::CompositeKeyCannotAutoincrement
            ));
        }
    }

    #[sqlx::test]
    async fn kv_objects(db: SqlitePool) {
        // Arrange
        let person = ModelBuilder::new("Person")
            .db("d1")
            .id_pk()
            .col("name", CidlType::Text, None, None)
            .col(
                "horseId",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Horse".into(),
                    column_name: "id".into(),
                }),
                None,
            )
            .nav_p(
                "horse",
                "Horse",
                NavigationPropertyKind::OneToOne {
                    key_columns: vec!["horseId".to_string()],
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
            .db("d1")
            .id_pk()
            .nav_p(
                "awards",
                "Award",
                NavigationPropertyKind::OneToMany {
                    key_columns: vec!["horseId".to_string()],
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
            .db("d1")
            .id_pk()
            .col(
                "horseId",
                CidlType::Integer,
                Some(ForeignKeyReference {
                    model_name: "Horse".into(),
                    column_name: "id".into(),
                }),
                None,
            )
            .col("title", CidlType::Text, None, None)
            // requires primary key to be set
            .kv_object(
                "award/{id}/certificate",
                "AWARD_KV",
                "certificate",
                false,
                CidlType::Text,
            )
            .build();

        let mut ast = create_ast(vec![person, horse, award]);
        ast.wrangler_env = Some(ast::WranglerEnv {
            name: "test".to_string(),
            source_path: PathBuf::default(),
            d1_bindings: vec!["d1".to_string()],
            kv_bindings: vec![
                "PERSON_KV".to_string(),
                "HORSE_KV".to_string(),
                "AWARD_KV".to_string(),
            ],
            r2_bindings: vec![],
            vars: HashMap::default(),
        });

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
