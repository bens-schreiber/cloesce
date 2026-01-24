use std::path::{Path, PathBuf};

use ast::{ApiMethod, CidlType, CloesceAst, CrudKind, HttpVerb, MediaType, NamedTypedValue};

// TODO: This is all hardcoded to TypeScript workers
pub struct WorkersGenerator;
impl WorkersGenerator {
    /// Generates all model source imports as well as the main source import
    ///
    /// Public for tests
    pub fn link(ast: &CloesceAst, workers_path: &Path) -> String {
        let workers_dir = workers_path
            .parent()
            .expect("workers_path has no parent; cannot compute relative imports");

        /// Tries to compute the relative path between two paths. If not possible, returns an empty err.
        fn rel_path(path: &Path, workers_dir: &Path) -> std::result::Result<String, ()> {
            // Remove the extension (e.g., .ts/.tsx/.js)
            let no_ext = path.with_extension("");

            // Compute the relative path from the workers file directory
            let rel = pathdiff::diff_paths(&no_ext, workers_dir).ok_or(())?;

            // Stringify + normalize to forward slashes
            let mut rel_str = rel.to_string_lossy().replace('\\', "/");

            // Ensure we have a leading './' when not starting with '../' or '/'
            if !rel_str.starts_with(['.', '/']) {
                rel_str = format!("./{}", rel_str);
            }

            // If we collapsed to empty (it can happen if model sits exactly at from_dir/index)
            if rel_str.is_empty() || rel_str == "." {
                rel_str = "./".to_string();
            }

            Ok(rel_str)
        }

        /// Generates import statements for a collection of source items
        fn imports<I, F>(items: I, workers_dir: &Path, f: F) -> String
        where
            I: IntoIterator,
            F: Fn(I::Item) -> (String, PathBuf),
        {
            items
                .into_iter()
                .map(|item| {
                    let (name, path) = f(item);
                    let path = rel_path(&path, workers_dir)
                        .unwrap_or_else(|_| path.to_string_lossy().to_string());
                    format!("import {{ {} }} from \"{}\";", name, path)
                })
                .collect::<Vec<_>>()
                .join("\n")
        }

        let main_import = match &ast.main_source {
            Some(p) => {
                let path = rel_path(p, workers_dir)
                    .unwrap_or_else(|_| p.clone().to_string_lossy().to_string());
                format!("import main from \"{path}\"")
            }
            None => String::default(),
        };

        [
            imports(&ast.models, workers_dir, |(name, model)| {
                (name.clone(), model.source_path.clone())
            }),
            imports(&ast.poos, workers_dir, |(name, poo)| {
                (name.clone(), poo.source_path.clone())
            }),
            imports(&ast.services, workers_dir, |(name, service)| {
                (name.clone(), service.source_path.clone())
            }),
            main_import,
        ]
        .join("\n")
    }

    /// Generates the constructor registry
    fn registry(ast: &CloesceAst) -> String {
        let symbols = ast
            .models
            .values()
            .map(|m| &m.name)
            .chain(ast.poos.values().map(|p| &p.name))
            .chain(ast.services.values().map(|s| &s.name));

        format!(
            "const constructorRegistry: Record<string, new () => any> = {{\n{}\n}};",
            symbols
                .map(|name| format!("\t{}: {}", name, name))
                .collect::<Vec<_>>()
                .join(",\n")
        )
    }

    /// Sets the [MediaType] of all ApiMethods; generates CRUD methods.
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

        let set_datasource_param = |method: &mut ApiMethod, model_name: &str| {
            if !method.is_static
                && !method
                    .parameters
                    .iter()
                    .any(|p| matches!(p.cidl_type, CidlType::DataSource(_)))
            {
                method.parameters.push(NamedTypedValue {
                    name: "__datasource".into(),
                    cidl_type: CidlType::DataSource(model_name.into()),
                });
            }
        };

        for model in ast.models.values_mut() {
            for crud in &model.cruds {
                let method = match crud {
                    CrudKind::GET => {
                        let mut parameters = vec![NamedTypedValue {
                            name: "__datasource".into(),
                            cidl_type: CidlType::DataSource(model.name.clone()),
                        }];

                        for key in &model.key_params {
                            parameters.push(NamedTypedValue {
                                name: key.clone(),
                                cidl_type: CidlType::Text,
                            });
                        }

                        if model.has_d1() {
                            let pk = model.primary_key.as_ref().expect("PK to exist");
                            parameters.push(NamedTypedValue {
                                name: pk.name.clone(),
                                cidl_type: pk.cidl_type.clone(),
                            });
                        }

                        // Data source should be last
                        parameters.reverse();

                        ApiMethod {
                            name: "GET".into(),
                            is_static: true,
                            http_verb: HttpVerb::GET,
                            return_type: CidlType::http(CidlType::Object(model.name.clone())),
                            parameters,
                            parameters_media: MediaType::default(),
                            return_media: MediaType::default(),
                        }
                    }
                    CrudKind::LIST => ApiMethod {
                        name: "LIST".into(),
                        is_static: true,
                        http_verb: HttpVerb::GET,
                        return_type: CidlType::http(CidlType::array(CidlType::Object(
                            model.name.clone(),
                        ))),
                        parameters: vec![NamedTypedValue {
                            name: "__datasource".into(),
                            cidl_type: CidlType::DataSource(model.name.clone()),
                        }],
                        parameters_media: MediaType::default(),
                        return_media: MediaType::default(),
                    },
                    CrudKind::SAVE => ApiMethod {
                        name: "SAVE".into(),
                        is_static: true,
                        http_verb: HttpVerb::POST,
                        return_type: CidlType::http(CidlType::Object(model.name.clone())),
                        parameters: vec![
                            NamedTypedValue {
                                name: "model".into(),
                                cidl_type: CidlType::Partial(model.name.clone()),
                            },
                            NamedTypedValue {
                                name: "__datasource".into(),
                                cidl_type: CidlType::DataSource(model.name.clone()),
                            },
                        ],
                        parameters_media: MediaType::default(),
                        return_media: MediaType::default(),
                    },
                };

                if model.methods.contains_key(&method.name) {
                    // Don't overwrite an existing method
                    tracing::warn!(
                        "Found an overwritten CRUD method {}.{}, skipping.",
                        model.name,
                        method.name
                    );
                    continue;
                }

                model.methods.insert(method.name.clone(), method);
            }

            for method in model.methods.values_mut() {
                set_datasource_param(method, &model.name);
                set_media_types(method);
            }
        }

        for service in ast.services.values_mut() {
            for method in service.methods.values_mut() {
                set_media_types(method);
            }
        }
    }

    pub fn generate(ast: &mut CloesceAst, workers_path: &Path) -> String {
        let linked_sources = Self::link(ast, workers_path);
        let constructor_registry = Self::registry(ast);

        Self::finalize_api_methods(ast);

        let fetch_impl = match &ast.main_source {
            Some(_) => "return await main(request, env, app, ctx);",
            None => "return await app.run(request, env);",
        };

        format!(
            r#"// GENERATED CODE. DO NOT MODIFY.
import {{ CloesceApp }} from "cloesce/backend";
import cidl from "./cidl.json";
{linked_sources}
{constructor_registry}

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {{
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    {fetch_impl}
}}

export default {{ fetch }};"#
        )
    }
}
