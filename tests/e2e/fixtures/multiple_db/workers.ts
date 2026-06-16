import { cloesce, DB1Model, DB2Model, CfEnv } from "./backend.js";

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = cloesce(env);
    app.register(DB1Model.impl({}), DB2Model.impl({}));
    return await app.run(request);
  },
};
