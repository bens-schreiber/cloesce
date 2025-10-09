import { HttpResult, Middleware, D1Database, WranglerEnv } from "cloesce";

@WranglerEnv
class Env {
  db: D1Database;
}

@Middleware
export class TestMiddleWare {
  async testMiddleware(): Promise<HttpResult<string>> {
    return { ok: true, data: "test" };
  }
}
