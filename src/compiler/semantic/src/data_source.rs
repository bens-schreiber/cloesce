use std::collections::HashSet;

use ast::{
    CidlType, CloesceAst, DataSource, DataSourceMethod, Field, IncludeTree, Model,
    NavigationFieldKind,
};

use codegen::orm::select::SelectModel;

pub struct DataSourceExpansion;
impl DataSourceExpansion {
    fn default_data_source(model: &Model, tree: IncludeTree, ast: &CloesceAst) -> DataSource {
        let Ok(include_sql) = SelectModel::query(&model.name, None, Some(tree.clone()), ast) else {
            // Model doesn't have any D1 fields, no SQL needed.
            return DataSource {
                name: "Default".into(),
                tree,
                is_internal: false,
                list: None,
                get: None,
            };
        };

        DataSource {
            name: "Default".into(),
            tree,
            is_internal: false,
            list: Some(Self::build_default_list(model, &include_sql)),
            get: Some(Self::build_default_get(model, &include_sql)),
        }
    }

    fn build_default_get(model: &Model, include_sql: &str) -> DataSourceMethod {
        let parameters = model
            .primary_columns
            .iter()
            .map(|pk| Field {
                name: pk.field.name.clone(),
                cidl_type: pk.field.cidl_type.clone(),
            })
            .chain(model.key_fields.iter().map(|key| Field {
                name: key.clone(),
                cidl_type: CidlType::String,
            }))
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
            .map(|_| "?".to_string())
            .collect::<Vec<String>>()
            .join(", ");

        let raw_sql = if model.primary_columns.len() == 1 {
            format!(
                r#"
                {include_sql}
                WHERE {where_clause} = ?
                "#
            )
        } else {
            format!(
                r#"
                {include_sql}
                WHERE ({where_clause}) = ({params})
                "#
            )
        };

        DataSourceMethod {
            parameters,
            raw_sql,
        }
    }

    fn build_default_list(model: &Model, include_sql: &str) -> DataSourceMethod {
        let parameters = model
            .primary_columns
            .iter()
            .map(|pk| Field {
                name: format!("lastSeen_{}", pk.field.name),
                cidl_type: CidlType::nullable(pk.field.cidl_type.clone()),
            })
            .chain(vec![Field {
                name: "limit".into(),
                cidl_type: CidlType::nullable(CidlType::Integer),
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
            .map(|_| "?".to_string())
            .collect::<Vec<String>>()
            .join(", ");

        let where_expr = if model.primary_columns.len() == 1 {
            format!("{where_clause} > ?")
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

        let raw_sql = format!(
            r#"
                {include_sql}
                WHERE {where_expr}
                ORDER BY {order}
                "#
        );

        DataSourceMethod {
            parameters,
            raw_sql,
        }
    }

    /// Resolves `$parameterName` placeholders in the raw SQL to positional `?N` syntax for prepared statements.
    pub fn resolve_sql_params(method: &mut DataSourceMethod) {
        for (i, param) in method.parameters.iter().enumerate() {
            let placeholder = format!("${}", param.name);
            let positional = format!("?{}", i + 1);
            method.raw_sql = method.raw_sql.replace(&placeholder, &positional);
        }
    }

    /// Creates a default [DataSource] for any model that doesn't have one,
    /// including default get/list SQL queries for models with D1 fields.
    ///
    /// Creates a default [IncludeTree] with all KV, R2, 1:1, 1:N and M:N relationships by default.
    /// Does not include relationships after a 1:N or M:N to avoid infinite trees.
    pub fn expand(ast: &mut CloesceAst) {
        // For each model without a default DS, build one
        {
            let models_to_process = ast
                .models
                .iter()
                .filter(|(_, model)| model.default_data_source().is_none())
                .map(|(_, model)| model.name.clone())
                .collect::<Vec<String>>();

            for model_name in models_to_process {
                let mut tree = IncludeTree::default();
                let mut visited = HashSet::new();
                dfs(ast, &model_name, &mut tree, &mut visited);

                let model = ast.models.get(&model_name).unwrap();
                let data_source = Self::default_data_source(model, tree, ast);

                ast.models
                    .get_mut(&model_name)
                    .unwrap()
                    .data_sources
                    .push(data_source);
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
                    .enumerate()
                    .map(|(i, ds)| {
                        let sql = SelectModel::query(&model.name, None, Some(ds.tree.clone()), ast)
                            .expect("select model to work");
                        let get = ds
                            .get
                            .is_none()
                            .then(|| Self::build_default_get(model, &sql));
                        let list = ds
                            .list
                            .is_none()
                            .then(|| Self::build_default_list(model, &sql));
                        (i, get, list, sql)
                    })
                    .collect();
                (model.name.clone(), defaults)
            })
            .collect::<Vec<(
                String,
                Vec<(
                    usize,
                    Option<DataSourceMethod>,
                    Option<DataSourceMethod>,
                    String,
                )>,
            )>>();

        for (name, defaults) in pending {
            let model = ast.models.get_mut(&name).unwrap();
            for (i, get, list, include_sql) in defaults {
                // Update the existing data source with missing get/list methods
                let ds = &mut model.data_sources[i];
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

        fn dfs(
            ast: &CloesceAst,
            current_model: &str,
            current_node: &mut IncludeTree,
            visited: &mut HashSet<String>,
        ) {
            if !visited.insert(current_model.to_string()) {
                return;
            }

            let model = ast.models.get(current_model).unwrap();
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

                        let mut new_node = IncludeTree::default();
                        dfs(ast, &nav.model_reference, &mut new_node, visited);
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
        }
    }
}
