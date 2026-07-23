import { createApp, Worker, CrudHaver, Parent, Child, type Api, type CfEnv } from "./backend.js";

const notCrud: Api.CrudHaver.notCrud = () => {};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker)
      .register(CrudHaver, { notCrud })
      .register(Parent, {})
      .register(Child, {})
      .run(request);
  },
};
