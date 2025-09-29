use std::{collections::HashMap, path::Path};

use common::{CidlSpec, CidlType, HttpVerb, Model, WranglerEnv, wrangler::WranglerSpec};

use anyhow::{Context, Result, anyhow, bail, ensure};

pub struct WorkersGenerator;
impl WorkersGenerator {
    /// Returns the API route
    fn validate_domain(domain: &str) -> Result<String> {
        if domain.is_empty() {
            return Err(anyhow!("Empty domain."));
        }

        match domain.split_once("://") {
            None => Err(anyhow!("Missing HTTP protocol")),
            Some((protocol, rest)) => {
                if protocol != "http" {
                    return Err(anyhow!("Unsupported protocol {}", protocol));
                }

                match rest.split_once("/") {
                    None => Err(anyhow!("Missing API route on domain")),
                    Some((_, rest)) => Ok(rest.to_string()),
                }
            }
        }
    }

    /// Validates all methods contain valid types and references.
    ///
    /// Returns error on
    /// - Unknown model reference on return type
    /// - Invalid parameter type on methods
    /// - Unknown model reference on method
    /// -
    fn validate_methods(models: &[Model]) -> Result<()> {
        let mut lookup = HashMap::<&str, &Model>::new();

        // TODO: We create a similiar lookup for D1 validation. It would be smart to pass that around.
        for model in models {
            lookup.insert(&model.name, model);
        }

        for model in models {
            for method in &model.methods {
                if let Some(Some(CidlType::Model(m))) =
                    method.return_type.as_ref().map(|r| r.root_type())
                {
                    ensure!(
                        lookup.contains_key(m.as_str()),
                        "Unknown model reference on model method return type {}.{}",
                        model.name,
                        method.name
                    );
                }

                for param in &method.parameters {
                    let Some(root_type) = param.cidl_type.root_type() else {
                        bail!(
                            "Invalid parameter type on model method {}.{}.{}",
                            model.name,
                            method.name,
                            param.name
                        );
                    };

                    if let CidlType::Model(m) = root_type {
                        ensure!(
                            lookup.contains_key(m.as_str()),
                            "Unknown model reference on model method {}.{}.{}",
                            model.name,
                            method.name,
                            param.name
                        );

                        if method.http_verb == HttpVerb::GET {
                            bail!(
                                "GET Requests currently do not support model parameters {}.{}.{}",
                                model.name,
                                method.name,
                                param.name
                            )
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Generates all model source imports
    fn link_models(models: &[Model], wenv: &WranglerEnv, workers_path: &Path) -> Result<String> {
        let workers_dir = workers_path
            .parent()
            .context("workers_path has no parent; cannot compute relative imports")?;

        fn rel_path(path: &Path, workers_dir: &Path) -> Result<String> {
            // Remove the extension (e.g., .ts/.tsx/.js)
            let no_ext = path.with_extension("");

            // Compute the relative path from the workers file directory
            let rel = pathdiff::diff_paths(&no_ext, workers_dir).ok_or_else(|| {
                anyhow!(
                    "Failed to compute relative path for '{}'\nfrom base '{}'",
                    path.display(),
                    workers_dir.display()
                )
            })?;

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

        let wrangler_env = format!(
            "import {{ {} }} from \"{}\"",
            wenv.name,
            rel_path(&wenv.source_path, workers_dir)?
        );

        let models = models
            .iter()
            .map(|m| -> Result<String> {
                rel_path(&m.source_path, workers_dir)
                    .map(|p| format!("import {{ {} }} from \"{}\";", m.name, p))
            })
            .collect::<Result<Vec<_>>>()?
            .join("\n");

        Ok(format!("{wrangler_env}\n{models}"))
    }

    /// Generates the constructor registry and instance registry
    fn registries(models: &[Model], wenv: &WranglerEnv) -> (String, String) {
        let mut constructor_registry = Vec::new();
        for model in models {
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
        cidl: CidlSpec,
        wrangler: WranglerSpec,
        domain: String,
        workers_path: &Path,
    ) -> Result<String> {
        Self::validate_methods(&cidl.models)?;
        let api_route = Self::validate_domain(&domain)?;

        // TODO: just hardcoding typescript for now
        let model_sources = Self::link_models(&cidl.models, &cidl.wrangler_env, workers_path)?;
        let (constructor_registry, instance_registry) =
            Self::registries(&cidl.models, &cidl.wrangler_env);

        // TODO: Hardcoding one database for now, in the future we need to support any amount
        let db_binding = wrangler
            .d1_databases
            .first()
            .context("A D1 database is required to run Cloesce")?
            .binding
            .as_ref()
            .context("A database needs a binding to reference it in the instance container")?;
        let env_name = &cidl.wrangler_env.name;

        // TODO: Middleware function should return the DI instance registry
        Ok(format!(
            r#"
import {{ cloesce }} from "cloesce";
import cidl from "./cidl.json" with {{ type: "json" }};
{model_sources}

{constructor_registry}

export default {{
    async fetch(request: Request, env: any, ctx: any): Promise<Response> {{
        {instance_registry}

        return await cloesce(cidl, constructorRegistry, instanceRegistry, request, "/{api_route}", {{ envName: "{env_name}", dbName: "{db_binding}" }});
    }}
}};
"#
        ))
    }
}
