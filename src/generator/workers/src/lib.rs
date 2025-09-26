use std::{collections::HashMap, path::Path};

use common::{CidlSpec, CidlType, HttpVerb, Model};

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

    fn constructor_registry(models: &[Model]) -> String {
        let mut entries = Vec::new();
        for model in models {
            let model_name = &model.name;
            entries.push(format!("  {}: {},", model_name, model_name));
        }
        let body = entries.join("\n");
        format!("const constructorRegistry = {{\n{}\n}};", body)
    }

    fn linker(models: &[Model], workers_path: &Path) -> Result<String> {
        let workers_dir = workers_path
            .parent()
            .context("workers_path has no parent; cannot compute relative imports")?;

        Ok(models
            .iter()
            .map(|m| -> Result<String> {
                // Remove the extension (e.g., .ts/.tsx/.js)
                let no_ext = m.source_path.with_extension("");

                // Compute the relative path from the workers file directory
                let rel = pathdiff::diff_paths(&no_ext, workers_dir).ok_or_else(|| {
                    anyhow!(
                        "Failed to compute relative path for '{}'\nfrom base '{}'",
                        m.source_path.display(),
                        workers_dir.display()
                    )
                })?;

                // Stringify + normalize to forward slashes
                let mut rel_str = rel.to_string_lossy().replace('\\', "/");

                // Ensure we have a leading './' when not starting with '../' or '/'
                if !rel_str.starts_with("../") && !rel_str.starts_with("./") {
                    rel_str = format!("./{}", rel_str);
                }

                // If we collapsed to empty (it can happen if model sits exactly at from_dir/index)
                if rel_str.is_empty() || rel_str == "." {
                    rel_str = "./".to_string();
                }

                Ok(format!("import {{ {} }} from '{}';", m.name, rel_str))
            })
            .collect::<Result<Vec<_>>>()?
            .join("\n"))
    }

    // TODO: just hardcoding typescript for now
    pub fn create(&self, cidl: CidlSpec, domain: String, workers_path: &Path) -> Result<String> {
        Self::validate_methods(&cidl.models)?;
        let api_route = Self::validate_domain(&domain)?;

        let linker = Self::linker(&cidl.models, workers_path)?;
        let registry = Self::constructor_registry(&cidl.models);

        // TODO: Use the correct DB name
        Ok(format!(
            r#"
import {{ cloesce }} from "cloesce";
import cidl from "./cidl.json" with {{ type: "json" }};
{linker}

{registry}

export default {{
    async fetch(request: Request, env: any, ctx: any): Promise<Response> {{
        return await cloesce(cidl, constructorRegistry, request, "/{api_route}", env.DB);
    }}
}};
"#
        ))
    }
}
