// Generated Cloudflare Workers API
// Version: 0.0.1
// Project: foo_proj

// IMPORTS
// Import generated models
import { User } from './models';

// PARAMETER VALIDATION FUNCTIONS

function validatedo_somethingParams(age: any, name: any) {
    if (age === null || age === undefined) {
        throw new Error('Required parameter missing: age');
    }
    if (age !== null && typeof age !== 'number') {
        throw new Error('Parameter age must be a number');
    }
    if (name === null || name === undefined) {
        throw new Error('Required parameter missing: name');
    }
    if (name !== null && typeof name !== 'string') {
        throw new Error('Parameter name must be a string');
    }
}

function validatesay_all_namesParams(age: any, name: any) {
    if (age === null || age === undefined) {
        throw new Error('Required parameter missing: age');
    }
    if (age !== null && typeof age !== 'number') {
        throw new Error('Parameter age must be a number');
    }
    if (name === null || name === undefined) {
        throw new Error('Required parameter missing: name');
    }
    if (name !== null && typeof name !== 'string') {
        throw new Error('Parameter name must be a string');
    }
}

// HTTP VERB VALIDATION FUNCTIONS

function validateGetMethod(request: Request): Response | null {
    // Check HTTP method
    if (request.method !== "GET") {
        return new Response("Method Not Allowed", { status: 405 });
    }
    return null;
}

// METHOD HANDLERS

const User_do_something_instance_handler = async (id: string, age: any, name: any, request: Request, env: any) => {
    try {
        // STAGE 1: HTTP Method Validation
    // Check HTTP method
    if (request.method !== "GET") {
        return new Response("Method Not Allowed", { status: 405 });
    }

        // STAGE 2: Parameter Validation
    if (age === null || age === undefined) {
        throw new Error('Required parameter missing: age');
    }
    if (age !== null && typeof age !== 'number') {
        throw new Error('Parameter age must be a number');
    }
    if (name === null || name === undefined) {
        throw new Error('Required parameter missing: name');
    }
    if (name !== null && typeof name !== 'string') {
        throw new Error('Parameter name must be a string');
    }

        // STAGE 3: Model Instantiation & Data Hydration
        const d1 = env.D1_DB || env.DB;
        
        // Query using the primary key field
        const query = `SELECT * FROM User WHERE id = ?`;
        const record = await d1.prepare(query).bind(id).first();
        
        if (!record) {
            return new Response(
                JSON.stringify({ error: "Record not found" }),
                { status: 404, headers: { "Content-Type": "application/json" } }
            );
        }
        
        // STAGE 4: Create Model Instance
        // The model class is imported, so we can instantiate it
        const instance = new User(record);
        
        // STAGE 5: Dependency Injection & Execute Instance Method
        const result = await instance.do_something(age, name);
        
        // STAGE 6: Return Response
        return new Response(JSON.stringify(result), {
            status: 200,
            headers: { "Content-Type": "application/json" }
        });
    } catch (error) {
        console.error("Error in User.do_something:", error);
        return new Response(
            JSON.stringify({ error: error.message }),
            { 
                status: error.status || 500,
                headers: { "Content-Type": "application/json" }
            }
        );
    }
};

const User_say_all_names_handler = async (age: any, name: any, request: Request, env: any) => {
    try {
        // STAGE 1: HTTP Method Validation
    // Check HTTP method
    if (request.method !== "GET") {
        return new Response("Method Not Allowed", { status: 405 });
    }

        // STAGE 2: Parameter Validation
    if (age === null || age === undefined) {
        throw new Error('Required parameter missing: age');
    }
    if (age !== null && typeof age !== 'number') {
        throw new Error('Parameter age must be a number');
    }
    if (name === null || name === undefined) {
        throw new Error('Required parameter missing: name');
    }
    if (name !== null && typeof name !== 'string') {
        throw new Error('Parameter name must be a string');
    }

        // STAGE 3: Dependency Injection
        const d1 = env.D1_DB || env.DB; // Support multiple binding names
        
        // STAGE 4: Execute Static Method
        // The model class is imported, so we can call static methods directly
        const result = await User.say_all_names(age, name);
        
        // STAGE 5: Return Response
        return new Response(JSON.stringify(result), {
            status: 200,
            headers: { "Content-Type": "application/json" }
        });
    } catch (error) {
        console.error("Error in User.say_all_names:", error);
        return new Response(
            JSON.stringify({ error: error.message }),
            { 
                status: error.status || 500,
                headers: { "Content-Type": "application/json" }
            }
        );
    }
};

// TYPE DEFINITIONS

type Handler = (...args: any[]) => Response;

// ROUTER STRUCTURE (TRIE)

// Trie-based router structure
const router = {
  api: {
    User: {
        "<id>": {
            do_something: User_do_something_instance_handler
        },
        say_all_names: User_say_all_names_handler
    }
  }
};
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
}

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
};