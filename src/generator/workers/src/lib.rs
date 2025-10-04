use std::{collections::BTreeMap, path::Path};

use common::{
    CloesceAst, Model, WranglerEnv,
    err::{GeneratorErrorKind, Result},
    fail,
};

use wrangler::WranglerSpec;

pub struct WorkersGenerator;
impl WorkersGenerator {
    /// Returns the API route
    fn validate_domain(domain: &str) -> Result<String> {
        if domain.is_empty() {
            fail!(GeneratorErrorKind::InvalidApiDomain, "Empty domain")
        }

        match domain.split_once("://") {
            None => fail!(GeneratorErrorKind::InvalidApiDomain, "Missing protocol"),
            Some((protocol, rest)) => {
                if protocol != "http" {
                    fail!(
                        GeneratorErrorKind::InvalidApiDomain,
                        "Unsupported protocol {}",
                        protocol
                    )
                }

                match rest.split_once("/") {
                    None => fail!(
                        GeneratorErrorKind::InvalidApiDomain,
                        "Missing API route on domain"
                    ),
                    Some((_, rest)) => Ok(rest.to_string()),
                }
            }
        }
    }

    /// Generates all model source imports
    fn link_models(models: &BTreeMap<String, Model>, workers_path: &Path) -> String {
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
            if !rel_str.starts_with("../") && !rel_str.starts_with("./") {
                rel_str = if rel_str.starts_with("/") {
                    format!(".{}", rel_str)
                } else {
                    format!("./{}", rel_str)
                }
            }

            // If we collapsed to empty (it can happen if model sits exactly at from_dir/index)
            if rel_str.is_empty() || rel_str == "." {
                rel_str = "./".to_string();
            }

            Ok(rel_str)
        }

        models
            .values()
            .map(|m| {
                // If the relative path is not possible, just use the file name.
                let p = rel_path(&m.source_path, workers_dir)
                    .unwrap_or_else(|_| m.source_path.clone().to_string_lossy().to_string());
                format!("import {{ {} }} from \"{}\";", m.name, p)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Generates the constructor registry and instance registry
    fn registries(models: &BTreeMap<String, Model>, wenv: &WranglerEnv) -> (String, String) {
        let mut constructor_registry = Vec::new();
        for model in models.values() {
            constructor_registry.push(format!("\t{}: {}", &model.name, &model.name));
        }

        let constructor_registry_def = {
            let body = constructor_registry.join(",\n");
            format!("const constructorRegistry = {{\n{}\n}};", body)
        };

        let instance_registry_def = format!(
            "const instanceRegistry = new Map([
            [\"{}\", env]
        ]);",
            wenv.name
        );

        (constructor_registry_def, instance_registry_def)
    }

    pub fn create(
        ast: CloesceAst,
        wrangler: WranglerSpec,
        domain: String,
        workers_path: &Path,
    ) -> Result<String> {
        let api_route = Self::validate_domain(&domain)?;

        // TODO: just hardcoding typescript for now
        let model_sources = Self::link_models(&ast.models, workers_path);
        let (constructor_registry, instance_registry) =
            Self::registries(&ast.models, &ast.wrangler_env);

        // TODO: Hardcoding one database for now, in the future we need to support any amount
        let db_binding = wrangler
            .d1_databases
            .first()
            .expect("A D1 database is required to run Cloesce")
            .binding
            .as_ref()
            .expect("A database needs a binding to reference it in the instance container");
        let env_name = &ast.wrangler_env.name;

        // TODO: Middleware function should return the DI instance registry
        Ok(format!(
            r#"
import {{ cloesce }} from "cloesce";
import cidl from "./cidl.json";
{model_sources}

{constructor_registry}

export default {{
    async fetch(request: Request, env: any, ctx: any): Promise<Response> {{
        {instance_registry}

        return await cloesce(request, cidl, constructorRegistry, instanceRegistry, {{ envName: "{env_name}", dbName: "{db_binding}" }},  "/{api_route}");
    }}
}};
"#
        ))
    }
}
