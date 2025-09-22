// typescript.rs
use common::{CidlType, HttpVerb, Method, Model, TypedValue};

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
}

pub struct TypescriptWorkersGenerator {
    domain: String,
    root_path: String,
}

impl TypescriptWorkersGenerator {
    pub fn new(domain: Option<String>) -> Self {
        let (normalized_domain, root) = Self::parse_domain(domain);
        Self {
            domain: normalized_domain,
            root_path: root,
        }
    }

    fn parse_domain(domain: Option<String>) -> (String, String) {
        let domain = domain.unwrap_or_else(|| "http://localhost:8787/api".to_string());
        
        // Normalize the domain - add http:// if no scheme is present
        let normalized = if !domain.starts_with("http://") && !domain.starts_with("https://") {
            format!("http://{}", domain)
        } else {
            domain.clone()
        };

        // Extract the path from the URL
        let root = if let Some(idx) = normalized.find("://") {
            // Skip the scheme
            let after_scheme = &normalized[idx + 3..];
            // Find the first '/' after the host
            if let Some(path_idx) = after_scheme.find('/') {
                // Get everything after the host
                let path = &after_scheme[path_idx + 1..];
                // Split by '/' and get the last non-empty segment
                path.split('/')
                    .filter(|s| !s.is_empty())
                    .last()
                    .unwrap_or("api")
                    .to_string()
            } else {
                // No path in the URL, use default
                "api".to_string()
            }
        } else {
            // No scheme found, try to parse as simple path
            normalized.split('/')
                .filter(|s| !s.is_empty() && !s.contains(':'))
                .last()
                .unwrap_or("api")
                .to_string()
        };

        (normalized, root)
    }


}

impl LanguageWorkersGenerator for TypescriptWorkersGenerator {
    fn imports(&self, models: &[Model]) -> String {
        let cf_types = r#"
import { D1Database } from "@cloudflare/workers-types"
"#;

        // TODO: Fix hardcoding path ../{}
        let model_imports = models
            .iter()
            .map(|m| {
                format!(
                    r#"
import {{ {} }} from '../{}'; 
"#,
                    m.name,
                    m.source_path.with_extension("").display() // strip the .ts off
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!("{cf_types}{model_imports}")
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
"#.to_string()
    }

    fn router(&self, model: String) -> String {
        format!(
            r#"
const router = {{ {root}: {{{model}}} }}
"#,
            root = self.root_path
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
        let method_name = &method.name;
        if method.is_static {
            format!(r#"{method_name}: {proto}"#)
        } else {
            format!(r#"{method_name}: {proto}"#)
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

    fn validate_req_body(&self, params: &[TypedValue]) -> String {
        let mut validate = Vec::new();

        let req_body_params = params
            .iter()
            .filter(|p| !matches!(p.cidl_type, CidlType::D1Database))
            .collect::<Vec<_>>();

        let invalid = r#"
            return new Response(JSON.stringify({ error: "Invalid request body" }), {
                status: 400,
                headers: { "Content-Type": "application/json" },
            });
        "#;

        // Instantiate from request body
        if !req_body_params.is_empty() {
            let req_body_params_lst = req_body_params
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
                .join(",");

            validate.push(format!(
                r#"
                let body;
                try {{
                    body = await request.json();
                }} catch {{
                    {invalid}
                }}

                let {{{req_body_params_lst}}} = body;
                "#
            ));
        }

        // Validate params from request body
        // Assign models to actual instances
        for param in req_body_params {
            if let Some(type_check) = TypescriptValidatorGenerator::validate_type(param, "") {
                validate.push(format!("if ({type_check}) {{ {invalid} }}"))
            }
            if let Some(assign) = TypescriptValidatorGenerator::assign_type(param) {
                validate.push(format!("{} = {assign}", param.name))
            }
        }

        validate.join("\n")
    }

    fn hydrate_model(&self, model: &Model) -> String {
        let model_name = &model.name;
        let pk = &model.find_primary_key().unwrap().name;

        // TODO: Switch based off DataSource type
        // For now, we will just assume there is a _default (or none)
        let has_ds = model.data_sources.len() > 1;

        let query = if has_ds {
            format!("`SELECT * FROM{model_name}_default WHERE {model_name}_{pk} = ?")
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