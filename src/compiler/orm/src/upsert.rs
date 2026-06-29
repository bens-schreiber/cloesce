use std::collections::HashMap;

use idl::{CidlType, CloesceIdl, IncludeTree, Model, NavigationCardinality, ValidatedField};

use frontend::fmt_cidl_type;
use sea_query::{Alias, OnConflict, SimpleExpr, SqliteQueryBuilder};
use sea_query::{Expr, Query};
use serde::{Deserialize, Serialize};
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
    pub kv_uploads: Vec<KvUpload>,
    pub kv_delayed_uploads: Vec<DelayedKvUpload>,
}

pub struct UpsertModel<'a> {
    idl: &'a CloesceIdl<'a>,
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
        idl: &'a CloesceIdl<'a>,
        new_model: Map<String, Value>,
        include_tree: Option<IncludeTreeJson>,
    ) -> Result<UpsertResult> {
        let include_tree = include_tree.unwrap_or_default();

        let mut generator = Self {
            idl,
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

        let model = idl.models.get(model_name).expect("Model to exist");
        if model.uses_sqlite() {
            // Final select to return the upserted model
            let select_query = {
                let include_tree_value = Value::Object(include_tree.clone());
                let include_tree_typed: IncludeTree =
                    IncludeTree::deserialize(&include_tree_value).unwrap_or_default();

                SelectModel::query(model_name, None, Some(&include_tree_typed), idl)?
                    .trim_start_matches("SELECT ")
                    .to_string()
            };

            let mut select = Query::select();
            let mut select_root_model = select.expr(Expr::cust(&select_query));

            // Add WHERE clause for each primary key column
            // e.g., WHERE "Model"."id" = (SELECT json_extract(primary_key, '$.id') FROM "$cloesce_tmp" WHERE path = 'Model')
            for col in &model.primary_columns {
                let pk_path = format!("{}.{}", model.name, col.field.name);
                let pk_expr = match generator.context.get(&pk_path) {
                    Some(Some(value)) => validate_and_transform(&col.field, value, idl)?,
                    _ => SqlUpsertBuilder::value_from_ctx(&pk_path),
                };
                select_root_model = select_root_model.and_where(
                    Expr::col((alias(model.name), alias(col.field.name.as_ref()))).eq(pk_expr),
                );
            }

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
        parent_model_name: Option<&str>,
        model_name: &str,
        mut new_model: Map<String, Value>,
        include_tree: &IncludeTreeJson,
        path: String,
    ) -> Result<()> {
        let model = match self.idl.models.get(model_name) {
            Some(m) => m,
            None => fail!(OrmErrorKind::UnknownModel {
                name: model_name.to_string(),
            }),
        };

        // KV Fields
        for kv in &model.kv_fields {
            let (value, metadata) = if kv.field.cidl_type.is_kv_object() {
                // Worker KV fields are wrapped as `KvObject<T>` (`{ raw, metadata }`).
                let Some(Value::Object(mut kv_object)) = new_model.remove(kv.field.name.as_ref())
                else {
                    fail!(OrmErrorKind::TypeMismatch {
                        expected: fmt_cidl_type(&kv.field.cidl_type),
                        got: Value::Null
                    })
                };

                let Some(value) = kv_object.remove("raw") else {
                    fail!(OrmErrorKind::MissingField {
                        missing: "raw".into(),
                        expected: fmt_cidl_type(&kv.field.cidl_type),
                    })
                };

                let metadata = kv_object.remove("metadata").unwrap_or(Value::Null);
                (value, metadata)
            } else {
                // Durable Object storage fields store their value directly, with no wrapper.
                let Some(value) = new_model.remove(kv.field.name.as_ref()) else {
                    fail!(OrmErrorKind::TypeMismatch {
                        expected: fmt_cidl_type(&kv.field.cidl_type),
                        got: Value::Null
                    })
                };
                (value, Value::Null)
            };

            let (key, placeholders_remain) =
                key_format_interpolation(&kv.key_format, &new_model, model)?;

            if placeholders_remain {
                let path_parts = path.split('.').skip(1).map(String::from).collect();
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

        if !model.uses_sqlite() {
            // Worker backed models have no SQL. Their navigation targets are sibling route models
            // assembled from this model's route fields, so persist their KV/R2 by recursing.
            for nav in &model.navigation_fields {
                let Some(Value::Object(nested_tree)) = include_tree.get(nav.field.name.as_ref())
                else {
                    continue;
                };
                let Some(Value::Object(nav_model)) = new_model.remove(nav.field.name.as_ref())
                else {
                    continue;
                };
                self.dfs(
                    Some(model.name),
                    nav.model_reference,
                    nav_model,
                    nested_tree,
                    format!("{path}.{}", nav.field.name),
                )?;
            }
            return Ok(());
        }

        let mut builder = SqlUpsertBuilder::new(model_name, self.idl);

        let (one_to_ones, others): (Vec<_>, Vec<_>) = model
            .navigation_fields
            .iter()
            .partition(|n| matches!(n.cardinality, NavigationCardinality::One));

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

            // Recursively handle nested inserts
            self.dfs(
                Some(model.name),
                nav.model_reference,
                nav_model,
                nested_tree,
                format!("{path}.{}", nav.field.name),
            )?;

            // Map each local key column to the path in the context where the target's
            // value can be found: `{path}.{nav}.{target}`.
            for key in &nav.keys {
                nav_ref_to_path.insert(
                    key.local,
                    format!("{path}.{}.{}", nav.field.name, key.target),
                );
            }
        }

        // If this model depends on another, its dependency will have been inserted
        // before this model. Thus, its parent pks exist in the context and can be used for FK resolution.
        let parent_id_paths = parent_model_name
            .map(|p| {
                let parent_path = path.rsplit_once('.').map(|(h, _)| h).unwrap_or(&path);
                self.idl
                    .models
                    .get(p)
                    .expect("Parent model not found in IDL")
                    .primary_columns
                    .iter()
                    .map(|pk_col| format!("{}.{}", parent_path, pk_col.field.name))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let pk_missing = model
            .primary_columns
            .iter()
            .all(|pk| !new_model.contains_key(pk.field.name.as_ref()));

        // Primary key columns and ordinary attributes resolve through the same
        // logic: a provided value, a context lookup (navigation or parent key),
        // an auto-incremented key, or a null default.
        for (attr, is_pk) in model.all_columns() {
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

            let new_model_value = new_model.remove(attr.field.name.as_ref());

            match (new_model_value, &attr.foreign_key_reference) {
                (Some(value), _) => {
                    // A value was provided in `new_model`
                    builder.push(&attr.field, is_pk, ColumnSource::Value(value));
                }

                (None, Some(_)) if path_key.is_some() => {
                    // No value provided, but this column is a FK reference
                    // resolved through context (a navigation or the parent key).
                    let path_key = path_key.unwrap();
                    let source = match self.context.get(path_key).expect("Context path missing") {
                        Some(value) => ColumnSource::Value(value.clone()),
                        None => ColumnSource::Context(path_key.clone()),
                    };

                    builder.push(&attr.field, is_pk, source);
                }

                (None, _) if is_pk && matches!(attr.field.cidl_type, CidlType::Int) => {
                    if model.has_composite_pk() {
                        fail!(OrmErrorKind::ModelKeyCannotAutoIncrement {
                            model: model_name.to_string(),
                            field: attr.field.name.to_string()
                        })
                    }

                    // The value is auto incremented and generated on insertion.
                    builder.push(&attr.field, is_pk, ColumnSource::AutoIncrement);
                }

                (None, _) if pk_missing && attr.field.cidl_type.is_nullable() => {
                    // Default to null for INSERT (no PK provided).
                    builder.push(&attr.field, is_pk, ColumnSource::Value(Value::Null));
                }

                (None, _) if !pk_missing && !is_pk => {
                    if !attr.field.cidl_type.is_nullable() {
                        // A non-nullable column is missing and we have a PK. This forces an UPDATE.
                        builder.flag_missing_non_nullable();
                    }
                }

                _ => fail!(OrmErrorKind::MissingField {
                    expected: fmt_cidl_type(&attr.field.cidl_type),
                    missing: attr.field.name.to_string(),
                }),
            };
        }

        // All sql dependencies have been resolved by this point.
        self.upsert_table(&path, builder)?;

        // Traverse navigation properties, using the include tree as a guide
        for nav in others {
            let Some(Value::Object(nested_tree)) = include_tree.get(nav.field.name.as_ref()) else {
                continue;
            };

            match (&nav.cardinality, new_model.remove(nav.field.name.as_ref())) {
                (NavigationCardinality::Many, Some(Value::Array(nav_models))) => {
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
                _ => {
                    // Ignore
                }
            }
        }

        Ok(())
    }

    /// Inserts the [SqlUpsertBuilder], updating the graph context to include the tables id.
    ///
    /// Returns an error if foreign key values exist that can not be resolved.
    fn upsert_table(&mut self, path: &str, builder: SqlUpsertBuilder) -> Result<()> {
        let all_pks_resolved = builder.all_pks_resolved();

        // Concrete primary key values are recorded so descendant rows can resolve
        // foreign keys that point at this row.
        let provided_pks = builder
            .provided_pk_values()
            .map(|(field, value)| (field.name.to_string(), value.clone()))
            .collect::<Vec<_>>();

        // The lone auto-incremented key (if any) whose value the database fills in.
        let auto_increment_pk = builder
            .columns
            .iter()
            .find(|c| c.is_pk && matches!(c.source, ColumnSource::AutoIncrement))
            .map(|c| c.field.name.to_string());

        self.sql_acc.push(builder.build()?);

        if all_pks_resolved {
            // Every key is known; store the concrete ones in context.
            for (pk_name, pk_val) in provided_pks {
                let id_path = format!("{path}.{pk_name}");
                self.context.insert(id_path, Some(pk_val));
            }

            return Ok(());
        }

        // The PK is not composite and is auto generated, we can retrieve the last
        // inserted rowid and store it in context.
        // The path for the JSON object is just the model path (not per-column)
        let pk_name = auto_increment_pk.expect("a single auto-incremented primary key");
        self.sql_acc
            .push(VariablesTable::insert_pk(path, &[(pk_name.clone(), None)]));

        // Mark PK column in context as needing lookup
        let id_path = format!("{path}.{pk_name}");
        self.context.insert(id_path, None);

        Ok(())
    }
}

const VARIABLES_TABLE_NAME: &str = "$cloesce_tmp";
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
                .replace(),
        )
    }
}

/// Where a resolved column gets its value from.
enum ColumnSource {
    /// A concrete JSON value (provided directly or a null default).
    Value(Value),
    /// A single-column integer primary key whose value is generated on insert.
    AutoIncrement,
    /// A value looked up from the graph context (a navigation property or the
    /// parent's primary key) via a `json_extract` subquery.
    Context(String),
}

/// A column resolved to its name, metadata and value source. Primary key columns
/// and ordinary attributes are resolved the same way and only diverge when the
/// final statement is assembled.
struct ResolvedColumn<'a> {
    field: &'a ValidatedField<'a>,
    is_pk: bool,
    source: ColumnSource,
}

struct SqlUpsertBuilder<'a> {
    model_name: &'a str,
    columns: Vec<ResolvedColumn<'a>>,
    idl: &'a CloesceIdl<'a>,
    has_missing_non_nullable: bool,
}

impl<'a> SqlUpsertBuilder<'a> {
    fn new(model_name: &'a str, idl: &'a CloesceIdl<'a>) -> SqlUpsertBuilder<'a> {
        Self {
            model_name,
            columns: Vec::default(),
            idl,
            has_missing_non_nullable: false,
        }
    }

    /// Records a resolved column.
    fn push(&mut self, field: &'a ValidatedField<'a>, is_pk: bool, source: ColumnSource) {
        self.columns.push(ResolvedColumn {
            field,
            is_pk,
            source,
        });
    }

    /// Force an UPDATE instead of an INSERT
    fn flag_missing_non_nullable(&mut self) {
        self.has_missing_non_nullable = true;
    }

    /// The primary key columns that carry a concrete value, paired with that
    /// value. Auto-increment and context-resolved keys are excluded.
    fn provided_pk_values(&self) -> impl Iterator<Item = (&'a ValidatedField<'a>, &Value)> {
        self.columns.iter().filter_map(|c| match &c.source {
            ColumnSource::Value(v) if c.is_pk => Some((c.field, v)),
            _ => None,
        })
    }

    /// True when no primary key is auto-incremented, i.e. every key has a value
    fn all_pks_resolved(&self) -> bool {
        !self
            .columns
            .iter()
            .any(|c| c.is_pk && matches!(c.source, ColumnSource::AutoIncrement))
    }

    /// Generates a subquery expression to retrieve a value from the context based on the path.
    fn value_from_ctx(path: &str) -> SimpleExpr {
        // "Model.columnName" => "columnName"
        let col_name = path.rsplit('.').next().unwrap_or(path);

        // Subquery to retrieve the value from the variables table based on the path.
        let base_path = &path[..path.rfind('.').unwrap_or(path.len())];
        let json_extract = format!(
            "(SELECT json_extract({}, '$.{}') FROM \"{}\" WHERE {} = '{}')",
            VARIABLES_TABLE_COL_PRIMARY_KEY,
            col_name,
            VARIABLES_TABLE_NAME,
            VARIABLES_TABLE_COL_PATH,
            base_path
        );

        SimpleExpr::Custom(json_extract)
    }

    /// Creates a SQL query, being either an update, insert, or upsert.
    fn build(self) -> Result<SqlStatement> {
        let column_expr = |col: &ResolvedColumn| {
            Ok(match &col.source {
                ColumnSource::Value(v) => Some(validate_and_transform(col.field, v, self.idl)?),
                ColumnSource::Context(path) => Some(Self::value_from_ctx(path)),
                ColumnSource::AutoIncrement => None,
            })
        };

        let any_pk_resolved = self.columns.iter().any(|c| c.is_pk);
        let all_pks_resolved = self.all_pks_resolved();

        // If we have PKs and a non-nullable column is missing, this must be an UPDATE
        // keyed on the (fully known) primary key.
        if any_pk_resolved && self.has_missing_non_nullable {
            let mut update = Query::update();
            let mut update_stmt = update.table(alias(self.model_name));

            for col in &self.columns {
                if let (false, Some(expr)) = (col.is_pk, column_expr(col)?) {
                    update_stmt = update_stmt.value(alias(col.field.name.as_ref()), expr);
                }
            }

            for (pk_field, pk_val) in self.provided_pk_values() {
                let expr = validate_and_transform(pk_field, pk_val, self.idl)?;
                update_stmt =
                    update_stmt.and_where(Expr::col(alias(pk_field.name.as_ref())).eq(expr));
            }

            return Ok(build_sqlite(update_stmt));
        }

        // Build an INSERT
        let mut insert = Query::insert();
        insert.into_table(alias(self.model_name));

        let mut cols = Vec::<Alias>::new();
        let mut vals = Vec::<SimpleExpr>::new();
        for col in &self.columns {
            if let Some(expr) = column_expr(col)? {
                cols.push(alias(col.field.name.as_ref()));
                vals.push(expr);
            }
        }

        if cols.is_empty() {
            insert.or_default_values();
        } else {
            insert.columns(cols).values_panic(vals);
        }

        // If the key is fully known and there are non-key columns, conflicting on
        // the primary key updates those columns (an upsert).
        let pk_names: Vec<&str> = self
            .columns
            .iter()
            .filter(|c| c.is_pk)
            .map(|c| c.field.name.as_ref())
            .collect();
        let update_cols: Vec<Alias> = self
            .columns
            .iter()
            .filter(|c| !c.is_pk)
            .map(|c| alias(c.field.name.as_ref()))
            .collect();

        if all_pks_resolved && !pk_names.is_empty() && !update_cols.is_empty() {
            insert.on_conflict(
                OnConflict::columns(pk_names.into_iter().map(alias))
                    .update_columns(update_cols)
                    .to_owned(),
            );
        }

        Ok(build_sqlite(&insert))
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

        let Some(param_value) = new_model.get(param_name) else {
            placeholders_remain = true;
            result.push_str(&format!("{{{}}}", param_name));
            last_end = end + 1;
            continue;
        };

        // Field is a column, primary key, or route field on the model.
        let field_meta = meta
            .all_columns()
            .map(|(col, _)| &col.field)
            .chain(meta.route_fields.iter())
            .find(|f| f.name == param_name)
            // guaranteed to exist by semantic analysis
            .unwrap();

        let replacement = match param_value {
            Value::String(s)
                if matches!(
                    field_meta.cidl_type.root_type(),
                    CidlType::String | CidlType::DateIso | CidlType::Json
                ) =>
            {
                s.clone()
            }
            Value::Number(n) if matches!(field_meta.cidl_type, CidlType::Real | CidlType::Int) => {
                n.to_string()
            }
            Value::Bool(b) if matches!(field_meta.cidl_type, CidlType::Boolean) => b.to_string(),
            _ => fail!(OrmErrorKind::TypeMismatch {
                expected: fmt_cidl_type(&field_meta.cidl_type),
                got: param_value.clone()
            }),
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

fn build_sqlite<T: sea_query::QueryStatementWriter>(qb: &T) -> SqlStatement {
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
    idl: &CloesceIdl,
) -> Result<SimpleExpr> {
    let res = validate_cidl_type(field, Some(value.clone()), idl, false);
    let value = match res {
        Ok(Some(v)) => v,
        Ok(None) => fail!(OrmErrorKind::MissingField {
            expected: fmt_cidl_type(&field.cidl_type),
            missing: field.name.to_string(),
        }),
        Err(e) => fail!(e),
    };

    Ok(match value {
        Value::Null => match field.cidl_type.root_type() {
            CidlType::Int => return Ok(Expr::val(None::<i32>).into()),
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
