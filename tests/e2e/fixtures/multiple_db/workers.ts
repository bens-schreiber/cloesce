import { cloesce, DB1Model, DB2Model, Env } from "./backend.js";

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const app = await cloesce();
    app.register(DB1Model.impl({})).register(DB2Model.impl({}));
    return await app.run(request, env);
  },
};
