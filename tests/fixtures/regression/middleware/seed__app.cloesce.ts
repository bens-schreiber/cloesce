import {
  CloesceApp,
  WranglerEnv,
  D1,
  PrimaryKey,
  CRUD,
  Inject,
  PlainOldObject,
  GET,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@PlainOldObject
export class InjectedThing {
  value: string;
}

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
@CRUD(["POST"])
export class Model {
  @PrimaryKey
  id: number;

  @GET
  static async blockedMethod() {}

  @GET
  static async getInjectedThing(
    @Inject thing: InjectedThing
  ): Promise<InjectedThing> {
    return thing;
  }
}

const app: CloesceApp = new CloesceApp();

app.useGlobal((request: Request, env, ir) => {
  if (request.method === "POST") {
    return { ok: false, status: 401, message: "POST methods aren't allowed." };
  }
});

app.useModel(Model, (request, env, ir) => {
  ir.set(InjectedThing.name, {
    value: "hello world",
  });
});

app.useMethod(Model, "blockedMethod", (request, env, ir) => {
  return { ok: false, status: 401, message: "Blocked method" };
});

export default app;
