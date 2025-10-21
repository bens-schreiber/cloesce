use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use common::{
    CloesceAst, Model, PlainOldObject,
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

    /// Generates all model source imports as well as the Cloesce App
    fn link(
        models: &BTreeMap<String, Model>,
        poos: &BTreeMap<String, PlainOldObject>,
        app_source: Option<&PathBuf>,
        workers_path: &Path,
    ) -> String {
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

        let model_imports = models
            .values()
            .map(|m| {
                // If the relative path is not possible, just use the file name.
                let path = rel_path(&m.source_path, workers_dir)
                    .unwrap_or_else(|_| m.source_path.clone().to_string_lossy().to_string());
                format!("import {{ {} }} from \"{}\";", m.name, path)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let poo_imports = poos
            .values()
            .map(|p| {
                // If the relative path is not possible, just use the file name.
                let path = rel_path(&p.source_path, workers_dir)
                    .unwrap_or_else(|_| p.source_path.clone().to_string_lossy().to_string());
                format!("import {{ {} }} from \"{}\";", p.name, path)
            })
            .collect::<Vec<_>>()
            .join("\n");

        let app_import = match app_source {
            Some(p) => {
                let path = rel_path(p, workers_dir)
                    .unwrap_or_else(|_| p.clone().to_string_lossy().to_string());
                format!("import app from \"{path}\"")
            }
            None => "const app = new CloesceApp()".into(),
        };

        format!("{model_imports}\n{poo_imports}\n{app_import}")
    }

    /// Generates the constructor registry
    fn registry(
        models: &BTreeMap<String, Model>,
        poos: &BTreeMap<String, PlainOldObject>,
    ) -> String {
        let mut constructor_registry = Vec::new();
        for model in models.values() {
            constructor_registry.push(format!("\t{}: {}", &model.name, &model.name));
        }

        for poo in poos.values() {
            constructor_registry.push(format!("\t{}: {}", &poo.name, &poo.name));
        }

        format!(
            "const constructorRegistry = {{\n{}\n}};",
            constructor_registry.join(",\n")
        )
    }

    pub fn create(
        ast: CloesceAst,
        wrangler: WranglerSpec,
        domain: String,
        workers_path: &Path,
    ) -> Result<String> {
        let api_route = Self::validate_domain(&domain)?;

        let linked_sources = Self::link(
            &ast.models,
            &ast.poos,
            ast.app_source.as_ref(),
            workers_path,
        );
        let constructor_registry = Self::registry(&ast.models, &ast.poos);

        // TODO: Hardcoding one database, in the future we need to support any amount
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
            r#"import {{ cloesce, CloesceApp }} from "cloesce/backend";
import cidl from "./cidl.json";
{linked_sources}
{constructor_registry}

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {{
    try {{
        const envMeta = {{ envName: "{env_name}", dbName: "{db_binding}" }};
        const apiRoute = "/{api_route}";
        return await cloesce(
            request, 
            env,
            cidl, 
            app,
            constructorRegistry, 
            envMeta,  
            apiRoute
        );
    }} catch(e: any) {{
        return new Response(JSON.stringify({{
            ok: false,
            status: 500,
            message: e.toString()
        }}), {{
            status: 500,
            headers: {{ "Content-Type": "application/json" }},
            }});
    }}
}}

export default {{fetch}};"#
        ))
    }
}
