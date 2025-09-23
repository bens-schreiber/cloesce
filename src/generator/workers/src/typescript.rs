use anyhow::anyhow;
use anyhow::{Context, Result};
use common::{CidlType, HttpVerb, Method, Model, TypedValue};
use std::path::Path;

use crate::LanguageWorkerGenerator as LanguageWorkersGenerator;

pub struct TypescriptValidatorGenerator;
impl TypescriptValidatorGenerator {
    fn validate_type(value: &TypedValue, name_prefix: &str) -> Option<String> {
        let name = format!("{name_prefix}{}", &value.name);

        let type_check = match &value.cidl_type {
            CidlType::Integer | CidlType::Real => Some(format!("typeof {name} !== \"number\"")),
            CidlType::Text => Some(format!("typeof {name} !== \"string\"")),
            CidlType::Blob => Some(format!(
                "!({name} instanceof ArrayBuffer || {name} instanceof Uint8Array)",
            )),
            CidlType::Model(m) => Some(format!("!$.{m}.validate({name})")),
            CidlType::Array(inner) => {
                let inner_value = TypedValue {
                    name: "item".to_string(),
                    cidl_type: *inner.clone(),
                    nullable: false,
                };
                let inner_check = Self::validate_type(&inner_value, name_prefix)?;
                Some(format!(
                    "!Array.isArray({name}) || {name}.some(item => {inner_check})",
                ))
            }
            _ => None,
        }?;

        let check = if value.nullable {
            format!("({name} !== undefined && {type_check})",)
        } else {
            format!("({name} == null || {type_check})",)
        };

        Some(check)
    }

    fn assign_type(value: &TypedValue) -> Option<String> {
        match &value.cidl_type {
            CidlType::Model(m) => Some(format!("Object.assign(new {}(), {})", m, value.name)),

            CidlType::Array(inner) => {
                let inner_ts = Self::assign_type(&TypedValue {
                    name: "item".to_string(),
                    cidl_type: *inner.clone(),
                    nullable: false,
                });

                inner_ts.map(|inner_code| format!("{}.map(item => {})", value.name, inner_code))
            }

            _ => None,
        }
    }

    fn validators(models: &[Model]) -> String {
        let mut validators = Vec::with_capacity(models.len());
        for model in models {
            let mut stmts = Vec::with_capacity(model.attributes.len());
            for attr in &model.attributes {
                let stmt = Self::validate_type(&attr.value, "obj.").expect("Valid method type");
                stmts.push(format!("if {stmt} {{return false;}}"))
            }

            let stmts = stmts.join("\n");
            let model_name = &model.name;
            validators.push(format!(
                r#"
                $.{model_name} = {{
                    validate(obj: any): boolean {{
                        {stmts}
                        return true;
                    }}
                }};
            "#
            ));
        }

        let validators = validators.join("\n");
        format!(
            r#"
            const $: any = {{}};

            {validators}
        "#
        )
    }
    pub fn extract_query_params(params: &[&TypedValue]) -> String {
        let extractions: Vec<String> = params
            .iter()
            .map(|p| {
                let name = &p.name;
                match &p.cidl_type {
                    CidlType::Integer => {
                        format!(
                            r#"
                const {name}Str = searchParams.get("{name}");
                if ({name}Str !== null) {{
                    const parsed = parseInt({name}Str, 10);
                    if (!isNaN(parsed)) {{
                        params.{name} = parsed;
                    }}
                }}"#
                        )
                    }
                    CidlType::Real => {
                        format!(
                            r#"
                const {name}Str = searchParams.get("{name}");
                if ({name}Str !== null) {{
                    const parsed = parseFloat({name}Str);
                    if (!isNaN(parsed)) {{
                        params.{name} = parsed;
                    }}
                }}"#
                        )
                    }
                    CidlType::Text => {
                        format!(
                            r#"
                const {name}Str = searchParams.get("{name}");
                if ({name}Str !== null) {{
                    params.{name} = {name}Str;
                }}"#
                        )
                    }
                    CidlType::Array(_) => {
                        format!(
                            r#"
                throw new Error("Array parameters are not supported in GET requests. Parameter: {name}");"#
                        )
                    }
                    CidlType::Model(_) => {
                        format!(
                            r#"
                throw new Error("Model parameters are not supported in GET requests. Parameter: {name}");"#
                        )
                    }
                    _ => {
                        // For other types, attempt to get as string
                        format!(
                            r#"
                const {name}Str = searchParams.get("{name}");
                if ({name}Str !== null) {{
                    params.{name} = {name}Str;
                }}"#
                        )
                    }
                }
            })
            .collect();

        extractions.join("\n                ")
    }
}

pub struct TypescriptWorkersGenerator;

impl LanguageWorkersGenerator for TypescriptWorkersGenerator {
    fn imports(&self, models: &[Model], workers_path: &Path) -> Result<String> {
        let cf_types = r#"
import { D1Database } from "@cloudflare/workers-types"
"#;

        let workers_dir = workers_path
            .parent()
            .context("workers_path has no parent; cannot compute relative imports")?;

        fn to_ts_import_path(abs_model_path: &Path, from_dir: &Path) -> Result<String> {
            // Remove the extension (e.g., .ts/.tsx/.js)
            let no_ext = abs_model_path.with_extension("");

            // Compute the relative path from the workers file directory
            let rel = pathdiff::diff_paths(&no_ext, from_dir).ok_or_else(|| {
                anyhow!(
                    "Failed to compute relative path for '{}'\nfrom base '{}'",
                    abs_model_path.display(),
                    from_dir.display()
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

            Ok(rel_str)
        }

        let model_imports = models
            .iter()
            .map(|m| -> Result<String> {
                let rel_str = to_ts_import_path(&m.source_path, workers_dir)?;
                Ok(format!("import {{ {} }} from '{}';", m.name, rel_str))
            })
            .collect::<Result<Vec<_>>>()?
            .join("\n");

        Ok(format!("{cf_types}\n{model_imports}\n"))
    }

    fn preamble(&self) -> String {
        include_str!("./templates/preamble.ts").to_string()
    }

    fn validators(&self, models: &[Model]) -> String {
        TypescriptValidatorGenerator::validators(models)
    }

    fn main(&self) -> String {
        r#"
export default {
    async fetch(request: Request, env: Env, ctx: any): Promise<Response> {
        try {
            const url = new URL(request.url);
            return await match(router, url.pathname, request, env);
        } catch (error: any) {
            console.error("Internal server error:", error);
            return new Response(JSON.stringify({ error: error?.message }), {
                status: 500,
                headers: { "Content-Type": "application/json" },
            });
        }
    }
};
"#
        .to_string()
    }

    fn router(&self, model: String) -> String {
        format!(
            r#"
const router = {{ api: {{{model}}} }}
"#
        )
    }

    fn router_model(&self, model_name: &str, method: String) -> String {
        format!(
            r#"
{model_name}: {{{method}}}
"#
        )
    }

    fn router_method(&self, method: &Method, proto: String) -> String {
        if method.is_static {
            proto
        } else {
            format!(
                r#"
"<id>": {{{proto}}}
"#
            )
        }
    }

    fn proto(&self, method: &Method, body: String) -> String {
        let method_name = &method.name;

        let id = if !method.is_static {
            "id: number, "
        } else {
            ""
        };

        format!(
            r#"
{method_name}: async ({id} request: Request, env: Env) => {{{body}}}
"#
        )
    }

    fn validate_http(&self, verb: &HttpVerb) -> String {
        let verb_str = match verb {
            HttpVerb::GET => "GET",
            HttpVerb::POST => "POST",
            HttpVerb::PUT => "PUT",
            HttpVerb::PATCH => "PATCH",
            HttpVerb::DELETE => "DELETE",
        };

        format!(
            r#"
if (request.method !== "{verb_str}") {{
    return new Response("Method Not Allowed", {{ status: 405 }});
}}
"#,
        )
    }

    fn validate_request(&self, method: &Method) -> String {
        let params = &method.parameters;

        let req_params = params
            .iter()
            .filter(|p| !matches!(p.cidl_type, CidlType::D1Database))
            .collect::<Vec<_>>();

        if req_params.is_empty() {
            return String::new();
        }

        let param_names = req_params
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ");

        let invalid_response = r#"
            return new Response(JSON.stringify({ error: "Invalid request parameters" }), {
                status: 400,
                headers: { "Content-Type": "application/json" },
            });
        "#;

        let mut validation_checks = Vec::new();
        let mut assignments = Vec::new();

        for param in &req_params {
            if let Some(type_check) = TypescriptValidatorGenerator::validate_type(param, "") {
                validation_checks.push(format!("if ({type_check}) {{ {invalid_response} }}"));
            }

            if let Some(assign) = TypescriptValidatorGenerator::assign_type(param) {
                assignments.push(format!("{} = {assign}", param.name));
            }
        }

        let validation_code = validation_checks.join("\n            ");
        let assignment_code = assignments.join(";\n            ");

        let extraction_logic = match method.http_verb {
            HttpVerb::GET => {
                let query_param_extractors =
                    TypescriptValidatorGenerator::extract_query_params(&req_params);
                format!(
                    r#"
            // Extract parameters from URL query string for GET request
            const url = new URL(request.url);
            const searchParams = url.searchParams;
            let params: any = {{}};
            
            {query_param_extractors}"#
                )
            }
            _ => {
                format!(
                    r#"
            // Extract parameters from request body for non-GET requests
            let params: any;
            try {{
                params = await request.json();
            }} catch {{
                {invalid_response}
            }}"#
                )
            }
        };

        format!(
            r#"
            {extraction_logic}
            
            let {{{param_names}}} = params;
            
            // Validate parameters
            {validation_code}
            
            // Assign model instances
            {assignment_code}
        "#
        )
    }

    fn hydrate_model(&self, model: &Model) -> String {
        let model_name = &model.name;
        let pk = &model.find_primary_key().unwrap().name;

        // TODO: Switch based off DataSource type
        // For now, we will just assume there is a _default (or none)
        let has_ds = model.data_sources.len() > 1;

        let query = if has_ds {
            format!("`SELECT * FROM {model_name}_default WHERE {model_name}_{pk} = ?`")
        } else {
            format!("`SELECT * FROM {model_name} WHERE {pk} = ?`")
        };

        let instance = if has_ds {
            format!("Object.assign(new {model_name}(), mapSql<{model_name}>(record)[0])")
        } else {
            format!("Object.assign(new {model_name}(), record)")
        };

        format!(
            r#"
const d1 = env.DB;
const query = {query};
const record = await d1.prepare(query).bind(id).first();
if (!record) {{
    return new Response(
        JSON.stringify({{ error: "Record not found" }}),
        {{ status: 404, headers: {{ "Content-Type": "application/json" }} }}
    );
}}
const instance = {instance};
"#
        )
    }

    fn dispatch_method(&self, model_name: &str, method: &Method) -> String {
        let method_name = &method.name;

        // SQL params are built by the body validator, CF params are dependency injected
        let params = method
            .parameters
            .iter()
            .map(|p| match &p.cidl_type {
                CidlType::D1Database => "env.DB".to_string(),
                _ => p.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");

        let callee = if method.is_static {
            model_name
        } else {
            "instance"
        };

        format!(
            r#"
return JSON.stringify({callee}.{method_name}({params}));
"#
        )
    }
}
