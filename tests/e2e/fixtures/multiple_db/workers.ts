import { createApp, DB1Model, DB2Model, type CfEnv } from "./backend.js";

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp().worker(env).register(DB1Model, {}).register(DB2Model, {}).run(request);
  },
};
