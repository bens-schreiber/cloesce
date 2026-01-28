import {
  CloesceApp,
  WranglerEnv,
  Model,
  Inject,
  GET,
  HttpResult,
  Integer,
} from "cloesce/backend";
import { D1Database, ExecutionContext } from "@cloudflare/workers-types";

export class InjectedThing {
  value: string;
}

@WranglerEnv
export class Env {
  db: D1Database;
}

@Model(["SAVE"])
export class Foo {
  id: Integer;

  @GET
  static blockedMethod() {}

  @GET
  static getInjectedThing(@Inject thing: InjectedThing): InjectedThing {
    return thing;
  }
}

export default async function main(
  request: Request,
  env: Env,
  app: CloesceApp,
  _ctx: ExecutionContext,
): Promise<Response> {
  if (request.method === "POST") {
    return HttpResult.fail(401, "POST methods aren't allowed.").toResponse();
  }

  app.onNamespace(Foo, (di) => {
    di.set(InjectedThing, {
      value: "hello world",
    });
  });

  app.onMethod(Foo, "blockedMethod", (_di) => {
    return HttpResult.fail(401, "Blocked method");
  });

  const result = await app.run(request, env);
  result.headers.set("X-Cloesce-Test", "true");

  return result;
}
