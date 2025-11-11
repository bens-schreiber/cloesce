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
type Integer = number & { __kind: "Integer" };

@PlainOldObject
export class InjectedThing {
  value: string;
}

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
@CRUD(["SAVE"])
export class Model {
  @PrimaryKey
  id: Integer;

  @GET
  static blockedMethod() {}

  @GET
  static getInjectedThing(@Inject thing: InjectedThing): InjectedThing {
    return thing;
  }
}

const app: CloesceApp = new CloesceApp();

app.onRequest((request: Request, env, ir) => {
  if (request.method === "POST") {
    return { ok: false, status: 401, message: "POST methods aren't allowed." };
  }
});

app.onModel(Model, (request, env, ir) => {
  ir.set(InjectedThing.name, {
    value: "hello world",
  });
});

app.onMethod(Model, "blockedMethod", (request, env, ir) => {
  return { ok: false, status: 401, message: "Blocked method" };
});

app.onResponse((request, env, ir, response: Response) => {
  response.headers.set("X-Cloesce-Test", "true");
});

export default app;
