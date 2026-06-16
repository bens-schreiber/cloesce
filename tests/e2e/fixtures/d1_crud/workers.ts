import { cloesce, CrudHaver, Parent, Child, CfEnv } from "./backend.js";

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = cloesce(env);
    app.register(CrudHaver.impl({ notCrud() {} }), Parent.impl({}), Child.impl({}));
    return await app.run(request);
  },
};
