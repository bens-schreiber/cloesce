import { WranglerEnv, CloesceApp, HttpResult } from "cloesce/backend";
import { D1Database, R2Bucket, ExecutionContext } from "@cloudflare/workers-types";

/**
 * Compiles to the Wrangler configuration file, defining bindings
 * for the Cloudflare Worker environment.
 */
@WranglerEnv
export class Env {
    db: D1Database;
    bucket: R2Bucket;
    myVariable: string;
}

// Basic main entry point for a Cloesce App.
// Does not need to be defined if no customizations are required.
export default async function main(
    request: Request,
    env: Env,
    app: CloesceApp,
    _ctx: ExecutionContext): Promise<Response> {
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
