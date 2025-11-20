use std::path::Path;

use ast::{
    ApiMethod, CidlType, CloesceAst, CrudKind, HttpVerb, MediaType, NamedTypedValue,
    semantic::BlobObjectSet,
};

use wrangler::WranglerSpec;

pub struct WorkersGenerator;
impl WorkersGenerator {
    /// Generates all model source imports as well as the Cloesce App
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

        let model_imports = ast
            .models
            .values()
            .map(|m| {
                // If the relative path is not possible, just use the file name.
                let path = rel_path(&m.source_path, workers_dir)
                    .unwrap_or_else(|_| m.source_path.clone().to_string_lossy().to_string());
                format!("import {{ {} }} from \"{}\";", m.name, path)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let poo_imports = ast
            .poos
            .values()
            .map(|p| {
                // If the relative path is not possible, just use the file name.
                let path = rel_path(&p.source_path, workers_dir)
                    .unwrap_or_else(|_| p.source_path.clone().to_string_lossy().to_string());
                format!("import {{ {} }} from \"{}\";", p.name, path)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let service_imports = ast
            .services
            .values()
            .map(|s| {
                // If the relative path is not possible, just use the file name.
                let path = rel_path(&s.source_path, workers_dir)
                    .unwrap_or_else(|_| s.source_path.clone().to_string_lossy().to_string());
                format!("import {{ {} }} from \"{}\";", s.name, path)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let app_import = match &ast.app_source {
            Some(p) => {
                let path = rel_path(p, workers_dir)
                    .unwrap_or_else(|_| p.clone().to_string_lossy().to_string());
                format!("import app from \"{path}\"")
            }
            None => "const app = new CloesceApp();".into(),
        };

        [model_imports, poo_imports, service_imports, app_import].join("\n")
    }

    /// Generates the constructor registry
    fn registry(ast: &CloesceAst) -> String {
        let mut constructor_registry = Vec::new();
        for model in ast.models.values() {
            constructor_registry.push(format!("\t{}: {}", &model.name, &model.name));
        }

        for poo in ast.poos.values() {
            constructor_registry.push(format!("\t{}: {}", &poo.name, &poo.name));
        }

        for service in ast.services.values() {
            constructor_registry.push(format!("\t{}: {}", &service.name, &service.name));
        }

        format!(
            "const constructorRegistry = {{\n{}\n}};",
            constructor_registry.join(",\n")
        )
    }

    /// Sets the [MediaType] of all ApiMethods; generates CRUD methods.
    ///
    /// Public for tests
    pub fn finalize_api_methods(ast: &mut CloesceAst, blob_objects: &BlobObjectSet) {
        for model in ast.models.values_mut() {
            for crud in &model.cruds {
                let method = match crud {
                    CrudKind::GET => ApiMethod {
                        name: "get".into(),
                        is_static: true,
                        http_verb: HttpVerb::GET,
                        return_type: CidlType::http(CidlType::Object(model.name.clone())),
                        parameters: vec![
                            NamedTypedValue {
                                name: model.primary_key.name.clone(),
                                cidl_type: model.primary_key.cidl_type.clone(),
                            },
                            NamedTypedValue {
                                name: "__datasource".into(),
                                cidl_type: CidlType::DataSource(model.name.clone()),
                            },
                        ],
                        parameters_media: MediaType::default(),
                        return_media: MediaType::default(),
                    },
                    CrudKind::LIST => ApiMethod {
                        name: "list".into(),
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
                        name: "save".into(),
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
                // Return Media Type
                method.return_media = match method.return_type.root_type() {
                    CidlType::Blob | CidlType::Stream => MediaType::Octet,
                    CidlType::Object(o) if blob_objects.contains(o) => MediaType::FormData,
                    _ => MediaType::Json,
                };

                let mut has_blob = false;
                let mut has_model = false;
                for param in &method.parameters {
                    match param.cidl_type.root_type() {
                        CidlType::Blob | CidlType::Stream => has_blob = true,
                        CidlType::Object(o) | CidlType::Partial(o) => {
                            has_blob |= blob_objects.contains(o);
                            has_model = true;
                        }
                        _ => {}
                    };
                }

                // Parameters Media
                method.parameters_media = match (has_blob, has_model) {
                    (true, true) => MediaType::FormData,
                    (true, false) => MediaType::Octet,
                    _ => MediaType::Json,
                };
            }
        }
    }

    pub fn create(
        ast: &mut CloesceAst,
        wrangler: WranglerSpec,
        workers_path: &Path,
        blob_objects: &BlobObjectSet,
    ) -> String {
        let linked_sources = Self::link(ast, workers_path);
        let constructor_registry = Self::registry(ast);

        Self::finalize_api_methods(ast, blob_objects);

        // TODO: Hardcoding one database, in the future we need to support any amount
        let db_binding = wrangler
            .d1_databases
            .first()
            .expect("A D1 database is required to run Cloesce")
            .binding
            .as_ref()
            .expect("A database needs a binding to reference it in the instance container");
        let env_name = &ast.wrangler_env.name;

        format!(
            r#"// GENERATED CODE. DO NOT MODIFY.
import {{ CloesceApp }} from "cloesce/backend";
import cidl from "./cidl.json";
{linked_sources}
{constructor_registry}

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {{
    const envMeta = {{ envName: "{env_name}", dbName: "{db_binding}" }};
    return await app.run(request, env, cidl as any, constructorRegistry, envMeta);
}}

export default {{ fetch }};"#
        )
    }
}
