import { D1Database, ExecutionContext } from "@cloudflare/workers-types";
import { CloesceApp, HttpResult, WranglerEnv } from "cloesce/backend";

@WranglerEnv
export class Env {
  db: D1Database;
  allowedOrigin: string;
}

export default async function main(
  request: Request,
  env: Env,
  app: CloesceApp,
  _ctx: ExecutionContext,
): Promise<Response> {

  // Preflight
  app.onRoute(async (di) => {
    const request = di.get(Request.name) as Request;

    if (request.method === "OPTIONS") {
      return HttpResult.ok(200, undefined, {
        "Access-Control-Allow-Origin": "*",
        "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS",
        "Access-Control-Allow-Headers": "Content-Type, Authorization",
      });
    }
  });

  const result = await app.run(request, env);

  result.headers.set("Access-Control-Allow-Origin", env.allowedOrigin);
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