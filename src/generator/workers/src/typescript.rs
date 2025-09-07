use client::LanguageTypeMapper;
use client::mappers::TypeScriptMapper;
use common::{CidlType, HttpVerb, Method, Model, TypedValue};

use crate::LanguageWorkerGenerator as LanguageWorkersGenerator;

pub struct TypescriptWorkersGenerator {}
impl LanguageWorkersGenerator for TypescriptWorkersGenerator {
    fn imports(&self, models: &[Model]) -> String {
        let model_names = models
            .iter()
            .map(|m| &m.name)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            r#"
        import {{ {model_names} }} from './models';
        "#
        )
    }

    fn preamble(&self) -> String {
        r#"
        function match(path: string, request: Request, env: any): Response {
            let node: any = router;
            const params: any[] = [];
            const segments = path.split("/").filter(Boolean);
            for (const segment of segments) {
                if (node[segment]) {
                    node = node[segment];
                } 
                else {
                    const paramKey = Object.keys(node).find(k => k.startsWith("<") && k.endsWith(">"));
                    if (paramKey) {
                        params.push(segment);
                        node = node[paramKey];
                    } else {
                        return new Response(
                            JSON.stringify({ error: "Route not found", path }),
                            { 
                                status: 404,
                                headers: { "Content-Type": "application/json" }
                            }
                        );
                    }
                }
            }
            if (typeof node === "function") {
                return node(...params, request, env);
            }
            return new Response(
                JSON.stringify({ error: "Route not found", path }),
                { 
                    status: 404,
                    headers: { "Content-Type": "application/json" }
                }
            );
        }
        "#.to_string()
    }

    fn main(&self) -> String {
        r#"
        export default {
            async fetch(request: Request, env: any, ctx: any): Promise<Response> {
                {}
                try {
                    const url = new URL(request.url);
                    return match(url.pathname, request, env);
                } catch (error) {
                    console.error("Worker error:", error);
                    return new Response(
                        JSON.stringify({ 
                            error: "Internal server error",
                            message: error.message 
                        }),
                        { 
                            status: 500,
                            headers: { "Content-Type": "application/json" }
                        }
                    );
                }
            }
        };
        "#
        .to_string()
    }

    fn router(&self, body: String) -> String {
        format!(
            r#"
        const router = {{ api: {{{body}}} }}
        "#
        )
    }

    fn router_model(&self, model_name: &str, body: String) -> String {
        format!(
            r#"
        {model_name}: {{{body}}}
        "#
        )
    }

    fn router_method(&self, method: &Method, body: String) -> String {
        let route = if method.is_static {
            &method.name
        } else {
            "<id>"
        };

        format!(
            r#"
        "{route}": {{{body}}}
        "#
        )
    }

    fn proto(&self, method: &Method, body: String) -> String {
        let method_name = &method.name;

        let params = method
            .parameters
            .iter()
            .map(|p| {
                format!(
                    "{}: {}",
                    p.name,
                    TypeScriptMapper.type_name(&p.cidl_type, p.nullable)
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        format!(
            r#"
            {method_name}: async ({params}, request: Request, env: any) => {{{body}}}
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

    fn validate_params(&self, params: &[TypedValue]) -> String {
        fn fmt_error(name: &String, ty: &str) -> String {
            format!(
                "if ({name} !== null && typeof {name} !== '{ty}') {{ throw new Error('Parameter {name} must be a {ty}'); }}",
            )
        }

        let mut validations = Vec::new();

        for param in params {
            if !param.nullable {
                validations.push(format!(
                    "if ({} === null || {} === undefined) {{ throw new Error('Required parameter missing: {}');}}",
                    param.name, param.name, param.name
                ));
            }

            let type_check = match param.cidl_type {
                CidlType::Integer | CidlType::Real => fmt_error(&param.name, "number"),
                CidlType::Text => fmt_error(&param.name, "string"),
                CidlType::Blob => {
                    format!(
                        "if ({} !== null && !({} instanceof ArrayBuffer || {} instanceof Uint8Array)) {{ throw new Error('Parameter {} must be a Uint8Array'); }}",
                        param.name, param.name, param.name, param.name
                    )
                }
            };

            validations.push(type_check);
        }

        if validations.is_empty() {
            String::from("")
        } else {
            validations.join("\n")
        }
    }

    fn instantiate_model(&self, model_name: &str) -> String {
        format!(
            r#"
        const d1 = env.D1_DB || env.DB;

        const query = `SELECT * FROM {model_name} WHERE id = ?`;
        const record = await d1.prepare(query).bind(id).first();

        if (!record) {{
            return new Response(
                JSON.stringify({{ error: "Record not found" }}),
                {{ status: 404, headers: {{ "Content-Type": "application/json" }} }}
            );
        }}

        const instance = new Person(record);
        "#
        )
    }

    fn dispatch_method(&self, model_name: &str, method: &Method) -> String {
        let method_name = &method.name;
        let params = method
            .parameters
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        let callee = if method.is_static {
            model_name
        } else {
            "instance"
        };

        format!(
            r#"
        {callee}.{method_name}({params})
        "#
        )
    }
}
