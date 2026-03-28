use std::collections::HashSet;

use ast::{
    ApiMethod, CidlType, CloesceAst, CrudKind, DataSource, DataSourceMethod, Field, HttpVerb,
    IncludeTree, MediaType, Model, NavigationFieldKind,
};

use crate::orm::select::SelectModel;

// TODO: This is all hardcoded to TypeScript workers
pub struct WorkersGenerator;
impl WorkersGenerator {
    /// - Sets the [MediaType] of all ApiMethods
    /// - Generates CRUD methods
    ///
    /// Public for tests
    pub fn finalize_api_methods(ast: &mut CloesceAst) {
        let set_media_types = |method: &mut ApiMethod| {
            method.return_media = match method.return_type.root_type() {
                CidlType::Stream => MediaType::Octet,
                _ => MediaType::Json,
            };

            method.parameters_media = if method
                .parameters
                .iter()
                .any(|p| matches!(p.cidl_type.root_type(), CidlType::Stream))
            {
                MediaType::Octet
            } else {
                MediaType::Json
            };
        };

        for model in ast.models.values_mut() {
            let mut crud_methods = vec![];
            for crud in &model.cruds {
                let method = match crud {
                    CrudKind::Get => {
                        let mut seen = HashSet::new();
                        let mut parameters = vec![];

                        for ds in &model.data_sources {
                            if let Some(get) = &ds.get {
                                for param in &get.parameters {
                                    if seen.insert(param.name.clone()) {
                                        parameters.push(Field {
                                            name: param.name.clone(),
                                            cidl_type: CidlType::nullable(param.cidl_type.clone()),
                                        });
                                    }
                                }
                            }
                        }

                        parameters.push(Field {
                            name: "__datasource".into(),
                            cidl_type: CidlType::DataSource {
                                model_name: model.name.clone(),
                            },
                        });

                        ApiMethod {
                            name: "$get".into(),
                            is_static: true,
                            http_verb: HttpVerb::Get,
                            return_type: CidlType::http(CidlType::Object {
                                name: model.name.clone(),
                            }),
                            parameters,
                            parameters_media: MediaType::default(),
                            return_media: MediaType::default(),
                            data_source: None,
                        }
                    }
                    CrudKind::List => {
                        let mut seen = HashSet::new();
                        let mut parameters = vec![];

                        for ds in &model.data_sources {
                            if let Some(list) = &ds.list {
                                for param in &list.parameters {
                                    if seen.insert(param.name.clone()) {
                                        parameters.push(Field {
                                            name: param.name.clone(),
                                            cidl_type: CidlType::nullable(param.cidl_type.clone()),
                                        });
                                    }
                                }
                            }
                        }

                        parameters.push(Field {
                            name: "__datasource".into(),
                            cidl_type: CidlType::DataSource {
                                model_name: model.name.clone(),
                            },
                        });

                        ApiMethod {
                            name: "$list".into(),
                            is_static: true,
                            http_verb: HttpVerb::Get,
                            return_type: CidlType::http(CidlType::array(CidlType::Object {
                                name: model.name.clone(),
                            })),
                            parameters,
                            parameters_media: MediaType::default(),
                            return_media: MediaType::default(),
                            data_source: None,
                        }
                    }
                    CrudKind::Save => ApiMethod {
                        name: "$save".into(),
                        is_static: true,
                        http_verb: HttpVerb::Post,
                        return_type: CidlType::http(CidlType::Object {
                            name: model.name.clone(),
                        }),
                        parameters: vec![
                            Field {
                                name: "model".into(),
                                cidl_type: CidlType::Partial {
                                    object_name: model.name.clone(),
                                },
                            },
                            Field {
                                name: "__datasource".into(),
                                cidl_type: CidlType::DataSource {
                                    model_name: model.name.clone(),
                                },
                            },
                        ],
                        parameters_media: MediaType::default(),
                        return_media: MediaType::default(),
                        data_source: None,
                    },
                };

                crud_methods.push(method);
            }

            model.apis.extend(crud_methods);
            for method in model.apis.iter_mut() {
                set_media_types(method);
            }
        }

        for service in ast.services.values_mut() {
            for method in service.apis.iter_mut() {
                set_media_types(method);
            }
        }
    }

    fn default_data_source(model: &Model, tree: IncludeTree, ast: &CloesceAst) -> DataSource {
        let Ok(include_sql) = SelectModel::query(&model.name, None, Some(tree.clone()), ast) else {
            // Model doesn't have any D1 fields, no SQL needed.
            return DataSource {
                name: "Default".into(),
                tree,
                is_private: false,
                list: None,
                get: None,
            };
        };

        DataSource {
            name: "Default".into(),
            tree,
            is_private: false,
            list: Some(Self::build_default_list(model, &include_sql)),
            get: Some(Self::build_default_get(model, &include_sql)),
        }
    }

    /// Generates a default [DataSource] for any model that doesn't have one.
    /// Also ensures every existing data source has default get/list implementations
    /// if they don't already define them.
    ///
    /// Includes all KV, R2, 1:1, 1:N and M:N relationships by default.
    /// Does not include relationships after a 1:N or M:N to avoid infinite trees.
    ///
    /// Public for tests
    pub fn generate_default_data_sources(ast: &mut CloesceAst) {
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

        // For each data source that lacks a `get` or `list` method, fills in
        // the default implementation (primary key get, seek pagination list).
        // Only applies to models with D1 bindings.
        let fills: Vec<_> = ast
            .models
            .values()
            .filter(|m| m.has_d1())
            .flat_map(|model| {
                model.data_sources.iter().enumerate().filter_map(|(i, ds)| {
                    if ds.get.is_some() && ds.list.is_some() {
                        return None;
                    }
                    let sql =
                        SelectModel::query(&model.name, None, Some(ds.tree.clone()), ast).ok()?;
                    let get = ds
                        .get
                        .is_none()
                        .then(|| Self::build_default_get(model, &sql));
                    let list = ds
                        .list
                        .is_none()
                        .then(|| Self::build_default_list(model, &sql));
                    Some((model.name.clone(), i, get, list))
                })
            })
            .collect();

        for (name, i, get, list) in fills {
            let ds = &mut ast.models.get_mut(&name).unwrap().data_sources[i];
            if let Some(g) = get {
                ds.get = Some(g);
            }
            if let Some(l) = list {
                ds.list = Some(l);
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

    /// Generates a default `main.ts` worker entrypoint.
    /// Finalizes all [ApiMethod]s and generates default [DataSource]s as needed.
    pub fn generate(ast: &mut CloesceAst, worker_url: &str) -> String {
        Self::generate_default_data_sources(ast);
        Self::finalize_api_methods(ast);

        format!(
            r#"// GENERATED CODE. DO NOT MODIFY.
import {{ CloesceApp }} from "cloesce/backend";
import cidl from "./cidl.json";

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {{
    const app = await CloesceApp.init(cidl as any, "{worker_url}");
    return await app.run(request, env);
}}

export default {{ fetch }};"#
        )
    }
}
