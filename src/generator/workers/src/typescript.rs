use common::{CidlType, HttpVerb, Method, Model, TypedValue};

use crate::WorkersGeneratable as LanguageWorkersGenerator;

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
            let stmts = model
                .attributes
                .iter()
                .filter_map(|attr| Self::validate_type(&attr.value, "obj."))
                .map(|stmt| format!("if {stmt} {{return false;}}"))
                .collect::<Vec<_>>()
                .join("\n");

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

pub struct TypescriptWorkersGenerator;
impl LanguageWorkersGenerator for TypescriptWorkersGenerator {
    fn imports(&self, models: &[Model]) -> String {
        const CLOUDFLARE_TYPES: &str = r#"import { D1Database } from "@cloudflare/workers-types""#;

        // TODO: Fix hardcoding path ../{}
        let model_imports = models
            .iter()
            .map(|model| {
                format!(
                    "import {{ {} }} from '../{}';",
                    model.name,
                    model.source_path.with_extension("").display()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        format!("{CLOUDFLARE_TYPES}\n{model_imports}")
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
        format!("const router = {{ api: {{{model}}} }}")
    }

    fn router_model(&self, model_name: &str, method: String) -> String {
        format!("{model_name}: {{{method}}}")
    }

    fn router_method(&self, method: &Method, proto: String) -> String {
        if method.is_static {
            proto
        } else {
            format!("\"<id>\": {{{proto}}}")
        }
    }

    fn proto(&self, method: &Method, body: String) -> String {
        let method_name = &method.name;
        let id_param = if method.is_static { "" } else { "id: number, " };

        format!("{method_name}: async ({id_param}request: Request, env: Env) => {{{body}}}")
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
        const INVALID_BODY_RESPONSE: &str = r#"
            return new Response(JSON.stringify({ error: "Invalid request body" }), {
                status: 400,
                headers: { "Content-Type": "application/json" },
            });
        "#;

        let req_body_params: Vec<_> = params
            .iter()
            .filter(|p| !matches!(p.cidl_type, CidlType::D1Database))
            .collect();

        if req_body_params.is_empty() {
            return String::new();
        }

        let mut validation_code = Vec::new();

        let param_names = req_body_params
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(",");
        validation_code.push(format!(
            r#"
                let body;
                try {{
                    body = await request.json();
                }} catch {{
                    {INVALID_BODY_RESPONSE}
                }}

                let {{{param_names}}} = body;
                "#
        ));

        // Validate params from request body
        // Assign models to actual instances
        for param in req_body_params {
            if let Some(type_check) = TypescriptValidatorGenerator::validate_type(param, "") {
                validation_code.push(format!("if ({type_check}) {{ {INVALID_BODY_RESPONSE} }}"))
            }
            if let Some(assign) = TypescriptValidatorGenerator::assign_type(param) {
                validation_code.push(format!("{} = {assign}", param.name))
            }
        }

        validation_code.join("\n")
    }

    fn hydrate_model(&self, model: &Model) -> String {
        let model_name = &model.name;
        let pk = &model.find_primary_key().unwrap().name;

        // TODO: Switch based off DataSource type
        // For now, we will just assume there is a _default (or none)
        let has_data_sources = model.data_sources.len() > 1;

        let (query, instance_creation) = if has_data_sources {
            (
                format!("`SELECT * FROM {model_name}_default WHERE {model_name}_{pk} = ?`"),
                format!("Object.assign(new {model_name}(), mapSql<{model_name}>(record)[0])"),
            )
        } else {
            (
                format!("`SELECT * FROM {model_name} WHERE {pk} = ?`"),
                format!("Object.assign(new {model_name}(), record)"),
            )
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
                const instance = {instance_creation};
            "#
        )
    }

    fn dispatch_method(&self, model_name: &str, method: &Method) -> String {
        let method_name = &method.name;

        let params = method
            .parameters
            .iter()
            .map(|param| match &param.cidl_type {
                CidlType::D1Database => "env.DB".to_string(),
                _ => param.name.clone(),
            })
            .collect::<Vec<_>>()
            .join(", ");

        let caller = if method.is_static {
            model_name
        } else {
            "instance"
        };

        format!("return JSON.stringify({caller}.{method_name}({params}));")
    }
}
