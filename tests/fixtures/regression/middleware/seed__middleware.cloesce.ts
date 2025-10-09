import {
  D1,
  PrimaryKey,
  GET,
  HttpResult,
  Middleware,
  D1Database,
  WranglerEnv,
  Inject,
} from "cloesce";

@WranglerEnv
class Env {
  db: D1Database;
}

@D1
export class House {
  @PrimaryKey
  id: number;

  name: string;

  @GET
  static async get(@Inject { db }: Env, id: number): Promise<House> {
    let records = await db
      .prepare("SELECT * FROM House WHERE id = ?")
      .bind(id)
      .run();
    return records.results[0] as House;
  }
}

@Middleware
export class TestMiddleWare {
  async handle(): Promise<HttpResult<string>> {
    return this.testMiddleware();
  }

  async testMiddleware(): Promise<HttpResult<string>> {
    return { ok: true, data: "test" };
  }
}

export { House };
