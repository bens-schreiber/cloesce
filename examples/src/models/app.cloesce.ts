import { D1Database } from "@cloudflare/workers-types";
import { CloesceApp, WranglerEnv } from "cloesce/backend";

@WranglerEnv
export class Env {
  db: D1Database;
  allowedOrigin: string;
}

const app = new CloesceApp();

// basic CORS
app.onResponse(async (request, env: Env, di, response: Response) => {
  console.log(env.allowedOrigin);
  response.headers.set("Access-Control-Allow-Origin", env.allowedOrigin);
  response.headers.set(
    "Access-Control-Allow-Methods",
    "GET, POST, PUT, DELETE, OPTIONS"
  );
  response.headers.set(
    "Access-Control-Allow-Headers",
    "Content-Type, Authorization"
  );
});

export default app;
