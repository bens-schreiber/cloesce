import { createApp, Session, type Api, type CfEnv } from "./backend.js";
import { HttpResult } from "cloesce";

const login: Api.Session.login = (env, email) =>
  HttpResult.ok(200, { value: `${email}:${env.JWT_SECRET}` });

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp().worker(env).register(Session, { login }).run(request);
  },
};
