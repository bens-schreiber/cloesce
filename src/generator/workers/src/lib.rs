use std::path::Path;

use common::{CidlSpec, Model};

use anyhow::{Context, Result, anyhow};

pub struct WorkersGenerator;
impl WorkersGenerator {
    fn constructor_registry(models: &[Model]) -> String {
        let mut entries = Vec::new();
        for model in models {
            let model_name = &model.name;
            entries.push(format!("  {}: {},", model_name, model_name));
        }
        let body = entries.join("\n");
        format!("const constructorRegistry = {{\n{}\n}};", body)
    }

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

    // TODO: compile-time validation of methods still has to happen
    // TODO: just hardcoding typescript for now, probably change that when validation is implemented
    pub fn create(&self, spec: CidlSpec, domain: String, workers_path: &Path) -> Result<String> {
        let linker = Self::linker(&spec.models, workers_path)?;
        let registry = Self::constructor_registry(&spec.models);
        let api_route = Self::validate_domain(&domain)?;

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
