use common::{CidlType, HttpVerb, Method, Model, TypedValue};

use crate::LanguageWorkerGenerator as LanguageWorkersGenerator;

pub struct TypescriptWorkersGenerator {}
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

        format!("{cf_types}\n{model_imports}")
    }

    fn preamble(&self) -> String {
        include_str!("./templates/preamble.ts").to_string()
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

    fn validate_req_body(&self, params: &[TypedValue]) -> String {
        fn fmt_error(name: &String, ty: &str) -> String {
            format!(
                "if ({name} !== null && typeof {name} !== '{ty}') {{ throw new Error('Parameter {name} must be a {ty}'); }}",
            )
        }

        let mut validate = Vec::new();

        let valid_params = params
            .iter()
            .filter(|p| !matches!(p.cidl_type, CidlType::D1Database))
            .collect::<Vec<_>>();

        // Instantiate Request Body
        if !valid_params.is_empty() {
            validate.push(format!(
                r#"
let body;
try {{
    body = await request.json();
}} catch {{
    return new Response(JSON.stringify({{ error: "Invalid request body" }}), {{
        status: 400,
        headers: {{ "Content-Type": "application/json" }},
    }});
}}

const {{{}}} = body;
"#,
                valid_params
                    .iter()
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }

        // Validate Request Body
        for param in valid_params {
            if !param.nullable {
                validate.push(format!(
                    "if ({} === null || {} === undefined) {{ throw new Error('Required parameter missing: {}');}}",
                    param.name, param.name, param.name
                ));
            }

            match &param.cidl_type {
                CidlType::Integer | CidlType::Real => validate.push(fmt_error(&param.name, "number")),
                CidlType::Text => validate.push(fmt_error(&param.name, "string")),
                CidlType::Blob => {
                    validate.push(format!(
                        "if ({} !== null && !({} instanceof ArrayBuffer || {} instanceof Uint8Array)) {{ throw new Error('Parameter {} must be a Uint8Array'); }}",
                        param.name, param.name, param.name, param.name
                    ))
                },
                _ => {
                    // Skip any other params, they may be dependency injected
                }
            };
        }

        validate.join("\n")
    }

    fn instantiate_model(&self, model: &Model) -> String {
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
