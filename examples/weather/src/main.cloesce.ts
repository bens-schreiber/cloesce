import { HttpResult } from "cloesce";
import { ExecutionContext } from "@cloudflare/workers-types";
import { Cloesce } from "../.cloesce/backend";

export default async function fetch(request: Request, env: Cloesce.Env, _ctx: ExecutionContext): Promise<Response> {
    // preflight
    if (request.method === "OPTIONS") {
        return HttpResult.ok(200, undefined, {
            "Access-Control-Allow-Origin": "*",
            "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS",
            "Access-Control-Allow-Headers": "Content-Type, Authorization",
        }).toResponse();
    }

    // Run Cloesce app
    const result = await app.run(request, env);

    // attach CORS headers
    result.headers.set("Access-Control-Allow-Origin", "*");
    result.headers.set(
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, DELETE, OPTIONS"
    );
    result.headers.set(
        "Access-Control-Allow-Headers",
        "Content-Type, Authorization"
    );

    return result;
}