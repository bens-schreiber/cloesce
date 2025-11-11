import { D1Database } from "@cloudflare/workers-types";
import { CloesceApp, HttpResult, WranglerEnv } from "cloesce/backend";

@WranglerEnv
export class Env {
  db: D1Database;
  allowedOrigin: string;
}

const app = new CloesceApp();

// Preflight
app.onRequest(async (request: Request, env, di) => {
  if (request.method === "OPTIONS") {
    return HttpResult.ok(204, undefined, {
      "Access-Control-Allow-Origin": "*",
      "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS",
      "Access-Control-Allow-Headers": "Content-Type, Authorization",
    });
  }
});

// attach CORS headers
app.onResult(async (request, env: Env, di, result: HttpResult) => {
  result.headers.set("Access-Control-Allow-Origin", env.allowedOrigin);
  result.headers.set(
    "Access-Control-Allow-Methods",
    "GET, POST, PUT, DELETE, OPTIONS"
  );
  result.headers.set(
    "Access-Control-Allow-Headers",
    "Content-Type, Authorization"
  );
});

export default app;
