use std::collections::{HashSet, VecDeque};

use ast::{
    CidlType, CloesceAst, DataSource, DataSourceMethod, Field, IncludeTree, Model,
    NavigationFieldKind,
};
use frontend::{DataSourceBlock, DataSourceBlockMethod, Symbol, SymbolKind};
use indexmap::IndexMap;
use orm::select::SelectModel;

use crate::{
    SymbolTable,
    err::{ErrorSink, SemanticError},
    is_valid_sql_type,
};

pub struct DataSourceAnalysis;
impl<'src, 'p> DataSourceAnalysis {
    pub fn analyze(
        data_source_blocks: &'p [DataSourceBlock<'src>],
        models: &IndexMap<&'src str, Model>,
        table: &SymbolTable<'src, 'p>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Vec<(&'src str, DataSource<'src>)> {
        let mut res = Vec::new();

        for ds in data_source_blocks {
            // Validate the model reference
            let Some(model_sym) = table.resolve(ds.model, SymbolKind::ModelDecl, None) else {
                sink.push(SemanticError::DataSourceUnknownModelReference { source: &ds.symbol });
                continue;
            };

            if !matches!(model_sym.kind, SymbolKind::ModelDecl) {
                sink.push(SemanticError::DataSourceUnknownModelReference { source: &ds.symbol });
                continue;
            }

            let model_name = model_sym.name;
            let Some(model) = models.get(model_name) else {
                // Model must be invalid for some reason, skip.
                continue;
            };

            // Validate include tree via BFS
            let mut q = VecDeque::new();
            q.push_back((&ds.tree, &model_name, model));

            while let Some((node, _parent_model_name, parent_model)) = q.pop_front() {
                for (field_name, child) in &node.0 {
                    // Check navigation properties
                    let nav = parent_model
                        .navigation_fields
                        .iter()
                        .find(|nav| &nav.field.name == field_name);

                    if let Some(nav) = nav {
                        // Navigate into the adjacent model
                        if let Some(adj_model) = models.get(nav.model_reference) {
                            q.push_back((child, &nav.model_reference, adj_model));
                        }
                        continue;
                    }

                    // Check KV properties
                    if parent_model
                        .kv_fields
                        .iter()
                        .any(|kv| &kv.field.name == field_name)
                    {
                        continue;
                    }

                    // Check R2 properties
                    if parent_model
                        .r2_fields
                        .iter()
                        .any(|r2| &r2.field.name == field_name)
                    {
                        continue;
                    }

                    sink.push(SemanticError::DataSourceInvalidIncludeTreeReference {
                        source: &ds.symbol,
                        model: model_name,
                        name: field_name.clone(),
                    });
                }
            }

            let list = ds
                .list
                .as_ref()
                .and_then(|m| Self::method(&ds.symbol, m, sink));
            let get = ds
                .get
                .as_ref()
                .and_then(|m| Self::method(&ds.symbol, m, sink));

            res.push((
                model_name,
                DataSource {
                    name: ds.symbol.name,
                    tree: IncludeTree(ds.tree.0.clone()),
                    list,
                    get,
                    is_internal: ds.is_internal,
                },
            ));
        }

        res
    }

    // Validate list and get method parameters
    fn method(
        source_sym: &'p Symbol<'src>,
        method: &'p DataSourceBlockMethod<'src>,
        sink: &mut ErrorSink<'src, 'p>,
    ) -> Option<DataSourceMethod<'src>> {
        let mut parameters = Vec::new();
        for param in &method.parameters {
            if !is_valid_sql_type(&param.cidl_type) {
                sink.push(SemanticError::DataSourceInvalidMethodParam {
                    source: source_sym,
                    param,
                });
            }
            parameters.push(Field {
                name: param.name.into(),
                cidl_type: param.cidl_type.clone(),
            });
        }

        // Verify every $name placeholder in the SQL is either $include or matches a parameter.
        let param_names: std::collections::HashSet<String> =
            parameters.iter().map(|p| p.name.to_string()).collect();
        let mut chars = method.raw_sql.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '$' {
                let name: String =
                    std::iter::from_fn(|| chars.next_if(|c| c.is_alphanumeric() || *c == '_'))
                        .collect();
                if !name.is_empty() && name != "include" && !param_names.contains(&name) {
                    sink.push(SemanticError::DataSourceUnknownSqlParam {
                        source: source_sym,
                        name,
                    });
                }
            }
        }

        Some(DataSourceMethod {
            parameters,
            raw_sql: method.raw_sql.to_string(),
        })
    }
}

pub struct DataSourceExpansion;
impl<'src> DataSourceExpansion {
    fn default_data_source(
        model: &Model<'src>,
        tree: IncludeTree<'src>,
        ast: &CloesceAst,
    ) -> DataSource<'src> {
        let Ok(include_sql) = SelectModel::query(model.name, None, Some(tree.clone()), ast) else {
            // Model doesn't have any D1 fields, no SQL needed.
            return DataSource {
                name: "Default",
                tree,
                is_internal: false,
                list: None,
                get: None,
            };
        };

        DataSource {
            name: "Default",
            tree,
            is_internal: false,
            list: Some(Self::build_default_list(model, &include_sql)),
            get: Some(Self::build_default_get(model, &include_sql)),
        }
    }

    // Simple get by primary key
    fn build_default_get(model: &Model<'src>, include_sql: &String) -> DataSourceMethod<'src> {
        let parameters = model
            .primary_columns
            .iter()
            .map(|pk| Field {
                name: pk.field.name.clone(),
                cidl_type: pk.field.cidl_type.clone(),
            })
            .collect();

        let where_clause = if model.primary_columns.len() == 1 {
            let pk = &model.primary_columns[0];
            format!(r#""{}"."{}""#, model.name, pk.field.name)
        } else {
            model
                .primary_columns
                .iter()
                .map(|pk| format!(r#""{}"."{}""#, model.name, pk.field.name))
                .collect::<Vec<String>>()
                .join(", ")
        };

        let params = (0..model.primary_columns.len())
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<String>>()
            .join(", ");

        let raw_sql = if model.primary_columns.len() == 1 {
            format!("{include_sql} WHERE {where_clause} = ?1")
        } else {
            format!("{include_sql} WHERE ({where_clause}) = ({params})")
        };

        DataSourceMethod {
            parameters,
            raw_sql,
        }
    }

    // Seek pagination based on primary keys with a limit, ordered by primary key ascending.
    fn build_default_list(model: &Model<'src>, include_sql: &String) -> DataSourceMethod<'src> {
        let parameters = model
            .primary_columns
            .iter()
            .map(|pk| Field {
                name: format!("lastSeen_{}", pk.field.name).into(),
                cidl_type: pk.field.cidl_type.clone(),
            })
            .chain(vec![Field {
                name: "limit".into(),
                cidl_type: CidlType::Integer,
            }])
            .collect();

        let where_clause = if model.primary_columns.len() == 1 {
            let pk = &model.primary_columns[0];
            format!(r#""{}"."{}""#, model.name, pk.field.name)
        } else {
            model
                .primary_columns
                .iter()
                .map(|pk| format!(r#""{}"."{}""#, model.name, pk.field.name))
                .collect::<Vec<String>>()
                .join(", ")
        };

        let params = (0..model.primary_columns.len())
            .enumerate()
            .map(|(i, _)| format!("?{}", i + 1))
            .collect::<Vec<String>>()
            .join(", ");

        let where_expr = if model.primary_columns.len() == 1 {
            format!("{where_clause} > ?1")
        } else {
            format!("({where_clause}) > ({params})")
        };

        let order = model
            .primary_columns
            .iter()
            .map(|pk| format!(r#""{}"."{}""#, model.name, pk.field.name))
            .collect::<Vec<String>>()
            .join(" ASC, ")
            + " ASC";

        let limit_param = model.primary_columns.len() + 1;
        let raw_sql =
            format!("{include_sql} WHERE {where_expr} ORDER BY {order} LIMIT ?{limit_param}");

        DataSourceMethod {
            parameters,
            raw_sql,
        }
    }

    /// Normalizes input and resolves `$parameterName` placeholders
    /// to positional `?N` syntax for prepared statements.
    pub fn resolve_sql_params(method: &mut DataSourceMethod) {
        method.raw_sql = method
            .raw_sql
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        for (i, param) in method.parameters.iter().enumerate() {
            let placeholder = format!("${}", param.name);
            let positional = format!("?{}", i + 1);
            method.raw_sql = method.raw_sql.replace(&placeholder, &positional);
        }
    }

    /// Creates a default [DataSource] for any model that doesn't have one,
    /// including default get/list SQL queries for models with D1 fields.
    ///
    /// Each data source has a default [IncludeTree] with all KV, R2, 1:1, 1:N and M:N relationships by default.
    /// Does not include relationships after a 1:N or M:N to avoid explosion (of sql joins not the computer).
    pub fn expand(ast: &mut CloesceAst<'src>) {
        // For each model without a default DS, build one
        {
            let models_to_process = ast
                .models
                .iter()
                .filter(|(_, model)| model.default_data_source().is_none())
                .map(|(_, model)| model.name)
                .collect::<Vec<&str>>();

            for model_name in models_to_process {
                let data_source = {
                    let tree = Self::include_dfs(&ast.models, model_name, &mut HashSet::new());
                    let model = ast.models.get(&model_name).unwrap();
                    Self::default_data_source(model, tree, ast)
                };

                ast.models
                    .get_mut(model_name)
                    .unwrap()
                    .data_sources
                    .insert(data_source.name, data_source);
            }
        }

        // Fill in any missing get/list with the default implementation
        let pending = ast
            .models
            .values()
            .filter(|m| m.has_d1())
            .map(|model| {
                let defaults = model
                    .data_sources
                    .iter()
                    .map(|(ds_name, ds)| {
                        let sql = SelectModel::query(model.name, None, Some(ds.tree.clone()), ast)
                            .expect("select model to work");
                        let get = ds
                            .get
                            .is_none()
                            .then(|| Self::build_default_get(model, &sql));
                        let list = ds
                            .list
                            .is_none()
                            .then(|| Self::build_default_list(model, &sql));
                        (*ds_name, get, list, sql)
                    })
                    .collect();
                (model.name, defaults)
            })
            .collect::<Vec<(
                &str,
                Vec<(
                    &str,
                    Option<DataSourceMethod>,
                    Option<DataSourceMethod>,
                    String,
                )>,
            )>>();

        for (name, defaults) in pending {
            let model = ast.models.get_mut(name).unwrap();
            for (ds_name, get, list, include_sql) in defaults {
                // Update the existing data source with missing get/list methods
                let ds = model.data_sources.get_mut(ds_name).unwrap();
                if let Some(g) = get {
                    ds.get = Some(g);
                }
                if let Some(l) = list {
                    ds.list = Some(l);
                }

                // Expand $include then resolve $paramName -> ?N
                if let Some(m) = &mut ds.get {
                    m.raw_sql = m.raw_sql.replace("$include", &include_sql);
                    Self::resolve_sql_params(m);
                }
                if let Some(m) = &mut ds.list {
                    m.raw_sql = m.raw_sql.replace("$include", &include_sql);
                    Self::resolve_sql_params(m);
                }
            }
        }
    }

    fn include_dfs<'m>(
        models: &'m IndexMap<&'src str, Model<'src>>,
        current_model: &'src str,
        visited: &mut HashSet<&'src str>,
    ) -> IncludeTree<'src> {
        if !visited.insert(current_model) {
            return IncludeTree::default();
        }

        let mut current_node = IncludeTree::default();

        let model = models.get(current_model).unwrap();
        for nav in &model.navigation_fields {
            match nav.kind {
                NavigationFieldKind::OneToOne { .. } => {
                    if nav.model_reference == current_model {
                        // Self-referencing 1:1. Include but don't recurse.
                        current_node
                            .0
                            .insert(nav.field.name.clone(), IncludeTree::default());
                        continue;
                    }

                    if visited.contains(&nav.model_reference) {
                        // Skip to avoid circular reference
                        continue;
                    }

                    let new_node = Self::include_dfs(models, nav.model_reference, visited);
                    current_node.0.insert(nav.field.name.clone(), new_node);
                }
                NavigationFieldKind::OneToMany { .. } | NavigationFieldKind::ManyToMany => {
                    // Include the related model as a leaf, but don't recurse.
                    current_node
                        .0
                        .insert(nav.field.name.clone(), IncludeTree::default());
                }
            }
        }

        for kv in &model.kv_fields {
            current_node
                .0
                .insert(kv.field.name.clone(), IncludeTree::default());
        }

        for r2 in &model.r2_fields {
            current_node
                .0
                .insert(r2.field.name.clone(), IncludeTree::default());
        }

        visited.remove(current_model);
        current_node
    }
}
