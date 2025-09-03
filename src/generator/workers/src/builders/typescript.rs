use common::{CidlSpec, CidlType, HttpVerb, InputLanguage, Method, Model, WranglerSpec};
use anyhow::anyhow;

use crate::WorkersApiBuilder;
pub struct TsWorkersApiBuilder {
    cidl: CidlSpec,         // The input spec with models and methods
    wrangler: WranglerSpec, // Cloudflare configuration (D1 databases, etc.)
}

impl TsWorkersApiBuilder {
    /// Creates a new TypeScript builder instance
    pub fn new(cidl: CidlSpec, wrangler: WranglerSpec) -> Self {
        Self { cidl, wrangler }
    }

    fn generate_imports(&self) -> String {
        // Collect all model names
        let model_names = self
            .cidl
            .models
            .iter()
            .map(|m| &m.name)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");

        format!(
            r#"// Import generated models
import {{ {} }} from './models';
"#,
            model_names
        )
    }
    fn generate_parameter_validation(&self, method: &Method) -> String {
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

    fn generate_http_verb_validation(&self, verb: &HttpVerb) -> String {
        let verb_str = match verb {
            HttpVerb::Get => "GET",
            HttpVerb::Post => "POST",
            HttpVerb::Put => "PUT",
            HttpVerb::Patch => "PATCH",
            HttpVerb::Delete => "DELETE",
        };

        format!(
            r#"    // Check HTTP method
    if (request.method !== "{}") {{
        return new Response("Method Not Allowed", {{ status: 405 }});
    }}"#,
            verb_str
        )
    }

    fn generate_method_handler(&self, model: &Model, method: &Method) -> String {
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
                self.generate_http_verb_validation(&method.http_verb),
                self.generate_parameter_validation(method),
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
                self.generate_http_verb_validation(&method.http_verb),
                self.generate_parameter_validation(method),
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


    fn build_router_trie(&self) -> String {
        let mut router_entries = Vec::new();

        for model in &self.cidl.models {
            let mut routes = Vec::new();

            for method in &model.methods {
                let handler = self.generate_method_handler(model, method);

                if method.is_static {
                    // Static routes go directly under the model
                    // Example: /api/Person/count
                    routes.push(format!("        {}: {}", method.name, handler));
                } else {
                    // Instance routes need an ID parameter
                    // Example: /api/Person/123/speak
                    routes.push(format!(
                        r#"        "<id>": {{
            {}: {}
        }}"#,
                        method.name, handler
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
        format!(
            r#"// Trie-based router structure
const router = {{
  api: {{
{}
  }}
}};"#,
            router_entries.join(",\n")
        )
    }

    fn generate_match_function(&self) -> &str {
        r#"
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
    }

    fn generate_fetch_handler(&self) -> &str {
        r#"
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
    }
}


impl WorkersApiBuilder for TsWorkersApiBuilder {
    /// Main build method that orchestrates all generation stages
    fn build(&self) -> Result<String, anyhow::Error> {
        // Validate we're generating TypeScript
        if !matches!(self.cidl.language, InputLanguage::TypeScript) {
            return Err(anyhow!("Only TypeScript is currently supported"));
        }

        let imports = self.generate_imports(); 
        let router_trie = self.build_router_trie(); 
        let match_function = self.generate_match_function(); 
        let fetch_handler = self.generate_fetch_handler(); 

        // Combine all components into final output
        let output = format!(
            r#"// Generated Cloudflare Workers API
// Version: {}
// Project: {}

// IMPORTS

{}

// TYPE DEFINITIONS

type Handler = (...args: any[]) => Response;

// ROUTER STRUCTURE (TRIE)

{}

// ROUTE MATCHING LOGIC
{}


// WORKER ENTRY POINT
{}"#,
            self.cidl.version,
            self.cidl.project_name,
            imports,
            router_trie,
            match_function,
            fetch_handler
        );

        Ok(output)
    }
}