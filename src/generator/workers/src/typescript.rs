use common::{CidlType, HttpVerb, Model, ModelMethod, NamedTypedValue};

use crate::WorkersGeneratable as LanguageWorkersGenerator;

pub struct TypescriptValidatorGenerator;
impl TypescriptValidatorGenerator {
    fn validate_type(value: &NamedTypedValue, name_prefix: &str) -> Option<String> {
        let name = format!("{name_prefix}{}", &value.name);

        let type_check = match &value.cidl_type {
            CidlType::Integer | CidlType::Real => Some(format!("typeof {name} !== \"number\"")),
            CidlType::Text => Some(format!("typeof {name} !== \"string\"")),
            CidlType::Blob => Some(format!(
                "!({name} instanceof ArrayBuffer || {name} instanceof Uint8Array)",
            )),
            CidlType::Model(m) => Some(format!("!$.{m}.validate({name})")),
            CidlType::Array(inner) => {
                let inner_value = NamedTypedValue {
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
            format!("({name} == undefined || {type_check})",)
        } else {
            format!("({name} == null || {type_check})",)
        };

        Some(check)
    }

    fn assign_type(value: &NamedTypedValue) -> Option<String> {
        match &value.cidl_type {
            CidlType::Model(m) => Some(format!("Object.assign(new {}(), {})", m, value.name)),

            CidlType::Array(inner) => {
                let inner_ts = Self::assign_type(&NamedTypedValue {
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

fn error_state(status: u32, stage: &str, message: &str) -> String {
    format!(
        r#"
        return {{ ok: false, status: {status}, message: `{stage}: ${{{message}}}`}}
    "#
    )
}

pub struct TypescriptWorkersGenerator;
impl LanguageWorkersGenerator for TypescriptWorkersGenerator {
    fn imports(&self, models: &[Model]) -> String {
        const CLOUDFLARE_TYPES: &str = r#"import { D1Database } from "@cloudflare/workers-types""#;
        const CLOESCE_TYPES: &str = r#"import { mapSql, match, Result } from "cloesce""#;

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

        format!("{CLOUDFLARE_TYPES}\n{CLOESCE_TYPES}\n{model_imports}")
    }

    fn preamble(&self) -> String {
        // TODO: Generate environment
        r#"
        export interface Env {
            DB: D1Database;
        }
        "#
        .to_string()
    }

    fn validators(&self, models: &[Model]) -> String {
        TypescriptValidatorGenerator::validators(models)
    }

    fn main(&self) -> String {
        r#"
export default {
    async fetch(request: Request, env: Env, ctx: any): Promise<Response> {
        const url = new URL(request.url);
        return await match(router, url.pathname, request, env);
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

    fn router_method(&self, method: &ModelMethod, proto: String) -> String {
        if method.is_static {
            proto
        } else {
            format!("\"<id>\": {{{proto}}}")
        }
    }

    fn proto(&self, method: &ModelMethod, body: String) -> String {
        let method_name = &method.name;
        let id_param = if method.is_static { "" } else { "id: number, " };

        format!(
            "{method_name}: async ({id_param}request: Request, env: Env): Promise<Result<any>> => {{{body}}}"
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

        // Error state: any method outside of the allowed HTTP verbs will exit with 405.
        let method_not_allowed = error_state(405, "validate_http", "\"Method Not Allowed\"");
        format!(
            r#"
                if (request.method !== "{verb_str}") {{
                    {method_not_allowed}
                }}
            "#,
        )
    }

    fn validate_req_body(&self, params: &[NamedTypedValue]) -> String {
        let req_body_params: Vec<_> = params
            .iter()
            .filter(|p| !matches!(p.cidl_type, CidlType::D1Database))
            .collect();
        if req_body_params.is_empty() {
            // No parameters, no validation.
            return String::new();
        }

        // Error state: any missing parameter, body, or malformed input will exit with 400.
        let invalid_request_body =
            error_state(400, "validate_req_body", "\"Invalid Request Body\"");

        let mut validation_code = Vec::new();
        validation_code.push(format!(
            r#"
                let body;
                try {{
                    body = await request.json();
                }} catch {{
                    {invalid_request_body}
                }}

                let {{{}}} = body;
                "#,
            req_body_params
                .iter()
                .map(|p| p.name.clone())
                .collect::<Vec<_>>()
                .join(",")
        ));

        // Validate params from request body
        for param in req_body_params {
            if let Some(type_check) = TypescriptValidatorGenerator::validate_type(param, "") {
                validation_code.push(format!("if ({type_check}) {{ {invalid_request_body} }}"))
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

        // Error state: If the D1 database has been tweaked outside of Cloesce
        // resulting in a malformed query, exit with a 500.
        let malformed_query = error_state(
            500,
            "hydrate_model",
            "e instanceof Error ? e.message : String(e)",
        );

        // Error state: If no record is found for the id, return a 404
        let missing_record = error_state(404, "hydrate_model", "\"Record not found\"");

        format!(
            r#"
                const d1 = env.DB;
                const query = {query};
                let record;
                try {{
                    record = await d1.prepare(query).bind(id).first();
                    if (!record) {{
                        {missing_record}
                    }}
                }}
                catch (e) {{
                    {malformed_query}
                }}
                const instance = {instance_creation};
            "#
        )
    }

    fn dispatch_method(&self, model_name: &str, method: &ModelMethod) -> String {
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

        let dispatch = match method.return_type {
            None => "return { ok: true, data: undefined };".to_string(),
            Some(CidlType::HttpResult(_)) => {
                format!("return await {caller}.{method_name}({params});")
            }
            Some(_) => {
                format!(
                    "return {{ ok: true, data: JSON.stringify( await {caller}.{method_name}({params}))}}"
                )
            }
        };

        // Error state: Client code ran into an uncaught exception.
        let uncaught_exception = error_state(
            500,
            "dispatch_method",
            "e instanceof Error ? e.message : String(e)",
        );

        format!(
            r#"
            try {{
                {dispatch}
            }}
            catch (e) {{
                {uncaught_exception}
            }}
        "#
        )
    }
}
