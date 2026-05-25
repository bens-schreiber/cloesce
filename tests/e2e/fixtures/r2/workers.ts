import { D1BackedModel, Env, cloesce, CfReadableStream, bucket1 } from "./backend.js";

export const D1BackedModelImpl = D1BackedModel.impl({
  async uploadData(self, env, data: CfReadableStream) {
    const key = bucket1.data(self.id);
    await env.bucket1.put(key, data as any);
  },

  async uploadOtherData(self, env, data: CfReadableStream) {
    const key = bucket1.otherData(self.id);
    await env.bucket1.put(key, data as any);
  },
});

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const app = await cloesce();
    app.register(D1BackedModelImpl);
    return await app.run(request, env);
  },
};
