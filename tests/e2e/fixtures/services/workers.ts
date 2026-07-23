import { createApp, Worker, FooService, InjectedThing, type Api, type CfEnv } from "./backend.js";
import { HttpResult } from "cloesce";

declare module "./backend.js" {
  interface InjectedThing {
    value: string;
  }
}

const method: Api.FooService.method = (env) =>
  HttpResult.ok(200, `foo's invocation; injected: ${env.InjectedThing.value}`);

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker)
      .register(FooService, { method })
      .register(InjectedThing, { value: "injected value" })
      .run(request);
  },
};
