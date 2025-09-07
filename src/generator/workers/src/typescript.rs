use common::{CidlType, HttpVerb, Method, Model, TypedValue};

use crate::LanguageWorkerGenerator as LanguageWorkersGenerator;

pub struct TypescriptWorkersGenerator {}
impl LanguageWorkersGenerator for TypescriptWorkersGenerator {
    fn imports(&self, models: &[Model]) -> String {
        // TODO: Fix hardcoding path ../{}
        models
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
            .join("\n")
    }

    fn preamble(&self) -> String {
        r#"
        function match(router: any, path: string, request: Request, env: any): Response {
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
            format!("{proto}")
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
            {method_name}: async ({id} request: Request, env: any) => {{{body}}}
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
            params
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
                .join(",")
        ));

        for param in params {
            if !param.nullable {
                validate.push(format!(
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

            validate.push(type_check);
        }

        if validate.is_empty() {
            String::from("")
        } else {
            validate.join("\n")
        }
    }

    fn instantiate_model(&self, model: &Model) -> String {
        let model_name = &model.name;
        let instance = model
            .attributes
            .iter()
            .map(|a| a.value.name.clone())
            .collect::<Vec<_>>()
            .join(",");

        // explicitly create the model so that users don't need
        // to create a constructor
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

        const instance: {model_name} = {{{instance}}};
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
        return {callee}.{method_name}({params})
        "#
        )
    }
}
