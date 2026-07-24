import { createApp, Worker, HeaderService, type Api, type CfEnv } from "./backend.js";
import { HttpResult } from "cloesce";

const echo: Api.HeaderService.echo = (X_Tenant, payload) =>
  HttpResult.ok(200, `${X_Tenant}:${payload}`);

const count: Api.HeaderService.count = (X_Count) => HttpResult.ok(200, X_Count + 1);

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker).register(HeaderService, { echo, count }).run(request);
  },
};
