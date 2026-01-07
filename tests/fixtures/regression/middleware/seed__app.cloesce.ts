import {
  CloesceApp,
  WranglerEnv,
  D1,
  PrimaryKey,
  CRUD,
  Inject,
  PlainOldObject,
  GET,
  HttpResult,
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

@Model
@CRUD(["SAVE"])
export class Model {
  @PrimaryKey
  id: Integer;

  @GET
  static blockedMethod() { }

  @GET
  static getInjectedThing(@Inject thing: InjectedThing): InjectedThing {
    return thing;
  }
}

const app: CloesceApp = new CloesceApp();

app.onRequest((di) => {
  const request = di.get("Request") as Request;
  if (request.method === "POST") {
    return HttpResult.fail(401, "POST methods aren't allowed.");
  }
});

app.onNamespace(Model, (di) => {
  di.set(InjectedThing.name, {
    value: "hello world",
  });
});

app.onMethod(Model, "blockedMethod", (di) => {
  return HttpResult.fail(401, "Blocked method");
});

app.onResult((_di, result: HttpResult) => {
  result.headers.set("X-Cloesce-Test", "true");
});

export default app;
