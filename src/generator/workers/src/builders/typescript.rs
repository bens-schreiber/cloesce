use anyhow::anyhow;
use common::{CidlType, HttpVerb, Method, Model};

use crate::WorkersApiBuilder;

pub struct TsWorkersApiBuilder {
    stack: Vec<String>,
}

impl TsWorkersApiBuilder {
    pub fn new() -> Self {
        Self { stack: Vec::new() }
    }

    fn push(&mut self, content: String) {
        self.stack.push(content);
    }

    fn generate_parameter_validation_for_method(&self, method: &Method) -> String {
        let mut validations = Vec::new();

        for param in &method.parameters {
            // First check if required (non-nullable)
            if !param.nullable {
                validations.push(format!(
                    "    if ({} === null || {} === undefined) {{\n        throw new Error('Required parameter missing: {}');\n    }}",
                    param.name, param.name, param.name
                ));
            }

            // Then check type based on CIDL type
            let type_check = match param.cidl_type {
                CidlType::Integer => {
                    format!(
                        "    if ({} !== null && typeof {} !== 'number') {{\n        throw new Error('Parameter {} must be a number');\n    }}",
                        param.name, param.name, param.name
                    )
                }
                CidlType::Real => {
                    format!(
                        "    if ({} !== null && typeof {} !== 'number') {{\n        throw new Error('Parameter {} must be a number');\n    }}",
                        param.name, param.name, param.name
                    )
                }
                CidlType::Text => {
                    format!(
                        "    if ({} !== null && typeof {} !== 'string') {{\n        throw new Error('Parameter {} must be a string');\n    }}",
                        param.name, param.name, param.name
                    )
                }
                CidlType::Blob => {
                    format!(
                        "    if ({} !== null && !({} instanceof ArrayBuffer || {} instanceof Uint8Array)) {{\n        throw new Error('Parameter {} must be a blob');\n    }}",
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

    fn generate_http_verb_validation_for_verb(&self, verb: &HttpVerb) -> String {
        let verb_str = match verb {
            HttpVerb::GET => "GET",
            HttpVerb::POST => "POST",
            HttpVerb::PUT => "PUT",
            HttpVerb::PATCH => "PATCH",
            HttpVerb::DELETE => "DELETE",
        };

        format!(
            r#"    // Check HTTP method
    if (request.method !== "{}") {{
        return new Response("Method Not Allowed", {{ status: 405 }});
    }}"#,
            verb_str
        )
    }

    fn generate_method_handler_code(&self, model: &Model, method: &Method) -> String {
        // Build parameter list for the handler function
        let mut params = Vec::new();
        if !method.is_static {
            params.push("id: string".to_string());
        }
        for param in &method.parameters {
            params.push(format!("{}: any", param.name));
        }
        params.push("request: Request".to_string());
        params.push("env: any".to_string());
        let params_str = params.join(", ");

        // Build the parameter list for calling the actual method
        let method_params = method
            .parameters
            .iter()
            .map(|p| &p.name)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");

        if method.is_static {
            // Static method handler - no instance needed
            format!(
                r#"async ({}) => {{
    try {{
        // STAGE 1: HTTP Method Validation
{}

        // STAGE 2: Parameter Validation
{}

        // STAGE 3: Dependency Injection
        const d1 = env.D1_DB || env.DB; // Support multiple binding names
        
        // STAGE 4: Execute Static Method
        // The model class is imported, so we can call static methods directly
        const result = await {}.{}({});
        
        // STAGE 5: Return Response
        return new Response(JSON.stringify(result), {{
            status: 200,
            headers: {{ "Content-Type": "application/json" }}
        }});
    }} catch (error) {{
        console.error("Error in {}.{}:", error);
        return new Response(
            JSON.stringify({{ error: error.message }}),
            {{ 
                status: error.status || 500,
                headers: {{ "Content-Type": "application/json" }}
            }}
        );
    }}
}}"#,
                params_str,
                self.generate_http_verb_validation_for_verb(&method.http_verb),
                self.generate_parameter_validation_for_method(method),
                model.name,
                method.name,
                method_params,
                model.name,
                method.name
            )
        } else {
            // Instance method handler - needs model instantiation
            // Find the primary key field for this model
            let pk_field = model
                .attributes
                .iter()
                .find(|attr| attr.primary_key)
                .map(|attr| attr.value.name.clone())
                .unwrap_or("id".to_string());

            format!(
                r#"async ({}) => {{
    try {{
        // STAGE 1: HTTP Method Validation
{}

        // STAGE 2: Parameter Validation
{}

        // STAGE 3: Model Instantiation & Data Hydration
        const d1 = env.D1_DB || env.DB;
        
        // Query using the primary key field
        const query = `SELECT * FROM {} WHERE {} = ?`;
        const record = await d1.prepare(query).bind(id).first();
        
        if (!record) {{
            return new Response(
                JSON.stringify({{ error: "Record not found" }}),
                {{ status: 404, headers: {{ "Content-Type": "application/json" }} }}
            );
        }}
        
        // STAGE 4: Create Model Instance
        // The model class is imported, so we can instantiate it
        const instance = new {}(record);
        
        // STAGE 5: Dependency Injection & Execute Instance Method
        const result = await instance.{}({});
        
        // STAGE 6: Return Response
        return new Response(JSON.stringify(result), {{
            status: 200,
            headers: {{ "Content-Type": "application/json" }}
        }});
    }} catch (error) {{
        console.error("Error in {}.{}:", error);
        return new Response(
            JSON.stringify({{ error: error.message }}),
            {{ 
                status: error.status || 500,
                headers: {{ "Content-Type": "application/json" }}
            }}
        );
    }}
}}"#,
                params_str,
                self.generate_http_verb_validation_for_verb(&method.http_verb),
                self.generate_parameter_validation_for_method(method),
                model.name,
                pk_field,
                model.name,
                method.name,
                method_params,
                model.name,
                method.name
            )
        }
    }
}

impl Default for TsWorkersApiBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkersApiBuilder for TsWorkersApiBuilder {
    fn imports(&mut self, models: &[Model]) -> &mut Self {
        // Collect all model names
        let model_names = models
            .iter()
            .map(|m| &m.name)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");

        let content = format!(
            r#"// Import generated models
import {{ {} }} from './models';
"#,
            model_names
        );

        self.push(content);
        self
    }

    fn parameter_validation(&mut self, methods: &[Method]) -> &mut Self {
        if methods.is_empty() {
            return self;
        }

        let validation_functions: Vec<String> = methods
            .iter()
            .map(|method| {
                let validation_code = self.generate_parameter_validation_for_method(method);
                if validation_code.is_empty() {
                    format!(
                        "function validate{}Params() {{\n    // No validation needed\n}}",
                        method.name
                    )
                } else {
                    format!(
                        "function validate{}Params({}) {{\n{}\n}}",
                        method.name,
                        method
                            .parameters
                            .iter()
                            .map(|p| format!("{}: any", p.name))
                            .collect::<Vec<_>>()
                            .join(", "),
                        validation_code
                    )
                }
            })
            .collect();

        let content = format!(
            r#"
// PARAMETER VALIDATION FUNCTIONS

{}
"#,
            validation_functions.join("\n\n")
        );

        self.push(content);
        self
    }

    fn http_verb_validation(&mut self, verbs: &[HttpVerb]) -> &mut Self {
        if verbs.is_empty() {
            return self;
        }

        // Simple deduplication without requiring Eq/Hash
        let mut unique_verbs = Vec::new();
        for verb in verbs {
            let verb_name = match verb {
                HttpVerb::GET => "Get",
                HttpVerb::POST => "Post",
                HttpVerb::PUT => "Put",
                HttpVerb::PATCH => "Patch",
                HttpVerb::DELETE => "Delete",
            };

            // Check if we already have this verb
            if !unique_verbs.iter().any(|(_, name)| name == &verb_name) {
                unique_verbs.push((verb, verb_name));
            }
        }

        let validation_functions: Vec<String> = unique_verbs
            .iter()
            .map(|(verb, verb_name)| {
                format!(
                    "function validate{}Method(request: Request): Response | null {{\n{}\n    return null;\n}}",
                    verb_name,
                    self.generate_http_verb_validation_for_verb(verb)
                )
            })
            .collect();

        let content = format!(
            r#"
// HTTP VERB VALIDATION FUNCTIONS

{}
"#,
            validation_functions.join("\n\n")
        );

        self.push(content);
        self
    }

    fn method_handlers(&mut self, models: &[Model]) -> &mut Self {
        if models.is_empty() {
            return self;
        }

        let mut handlers = Vec::new();

        for model in models {
            for method in &model.methods {
                let handler_name = if method.is_static {
                    format!("{}_{}_handler", model.name, method.name)
                } else {
                    format!("{}_{}_instance_handler", model.name, method.name)
                };

                let handler_code = format!(
                    "const {} = {};",
                    handler_name,
                    self.generate_method_handler_code(model, method)
                );

                handlers.push(handler_code);
            }
        }

        let content = format!(
            r#"
// METHOD HANDLERS

{}
"#,
            handlers.join("\n\n")
        );

        self.push(content);
        self
    }

    fn router_trie(&mut self, models: &[Model]) -> &mut Self {
        if models.is_empty() {
            let content = r#"
// TYPE DEFINITIONS

type Handler = (...args: any[]) => Response;

// ROUTER STRUCTURE (TRIE)

// Trie-based router structure
const router = {
  api: {}
};
"#
            .to_string();
            self.push(content);
            return self;
        }

        let mut router_entries = Vec::new();

        for model in models {
            let mut routes = Vec::new();

            for method in &model.methods {
                let handler_name = if method.is_static {
                    format!("{}_{}_handler", model.name, method.name)
                } else {
                    format!("{}_{}_instance_handler", model.name, method.name)
                };

                if method.is_static {
                    // Static routes go directly under the model
                    // Example: /api/Person/count
                    routes.push(format!("        {}: {}", method.name, handler_name));
                } else {
                    // Instance routes need an ID parameter
                    // Example: /api/Person/123/speak
                    routes.push(format!(
                        r#"        "<id>": {{
            {}: {}
        }}"#,
                        method.name, handler_name
                    ));
                }
            }

            // Combine all routes for this model
            router_entries.push(format!(
                "    {}: {{\n{}\n    }}",
                model.name,
                routes.join(",\n")
            ));
        }

        // Build the complete router
        let content = format!(
            r#"
// TYPE DEFINITIONS

type Handler = (...args: any[]) => Response;

// ROUTER STRUCTURE (TRIE)

// Trie-based router structure
const router = {{
  api: {{
{}
  }}
}};"#,
            router_entries.join(",\n")
        );

        self.push(content);
        self
    }

    fn route_matcher(&mut self) -> &mut Self {
        let content = r#"
// ROUTE MATCHING LOGIC

function match(path: string, request: Request, env: any): Response {
    // Start at the router root
    let node: any = router;
    const params: any[] = [];
    
    // Split path into segments and filter out empty strings
    const segments = path.split("/").filter(Boolean);
    
    // Walk through each segment to traverse the trie
    for (const segment of segments) {
        // Try exact match first (most common case)
        if (node[segment]) {
            node = node[segment];
        } 
        // If no exact match, look for parameter placeholders
        else {
            // Find keys that look like parameters (e.g., "<id>")
            const paramKey = Object.keys(node).find(k => k.startsWith("<") && k.endsWith(">"));
            if (paramKey) {
                // Save the actual parameter value
                params.push(segment);
                node = node[paramKey];
            } else {
                // No match found - return 404
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
    
    // Check if we reached a handler function
    if (typeof node === "function") {
        // Call the handler with collected parameters plus request and env
        return node(...params, request, env);
    }
    
    // Path incomplete or no handler found
    return new Response(
        JSON.stringify({ error: "Route not found", path }),
        { 
            status: 404,
            headers: { "Content-Type": "application/json" }
        }
    );
}"#
        .to_string();

        self.push(content);
        self
    }

    fn fetch_handler(&mut self) -> &mut Self {
        let content = r#"

// WORKER ENTRY POINT

// Main Cloudflare Workers handler
export default {
    async fetch(request: Request, env: any, ctx: any): Promise<Response> {
        try {
            const url = new URL(request.url);
            
            // Route the request through our trie-based matcher
            return match(url.pathname, request, env);
        } catch (error) {
            // Global error handler for unexpected errors
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
};"#
        .to_string();

        self.push(content);
        self
    }

    fn header(&mut self, version: &str, project_name: &str) -> &mut Self {
        let content = format!(
            r#"// Generated Cloudflare Workers API
// Version: {}
// Project: {}

// IMPORTS
"#,
            version, project_name
        );

        // Insert at the beginning
        self.stack.insert(0, content);
        self
    }

    fn build(&self) -> Result<String, anyhow::Error> {
        if self.stack.is_empty() {
            return Err(anyhow!("No content generated - builder stack is empty"));
        }

        Ok(self.stack.join(""))
    }
}
