import { cloesce, CrudHaver, Parent, Child, Env } from "./backend.js";

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const app = await cloesce();
    app.register(CrudHaver.impl({ notCrud() {} }));
    app.register(Parent.impl({}));
    app.register(Child.impl({}));
    return await app.run(request, env);
  },
};
