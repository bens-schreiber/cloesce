import { D1BackedModel, R2Only, R2Sibling, CfEnv, cloesce, CfReadableStream } from "./backend.js";

export const D1BackedModelImpl = D1BackedModel.impl({
  async uploadData(self, env, data: CfReadableStream) {
    await env.bucket1.data.put(self.id, data as any);
  },

  async uploadOtherData(self, env, data: CfReadableStream) {
    await env.bucket1.otherData.put(self.id, data as any);
  },
});

export const R2OnlyImpl = R2Only.impl({
  async uploadData(self, env, data: CfReadableStream) {
    await env.bucket1.data.put(self.id, data as any);
  },
});

export const R2SiblingImpl = R2Sibling.impl({
  async uploadData(self, env, data: CfReadableStream) {
    await env.bucket1.otherData.put(self.siblingId, data as any);
  },
});

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = cloesce(env);
    app.register(D1BackedModelImpl);
    app.register(R2OnlyImpl);
    app.register(R2SiblingImpl);
    return await app.run(request);
  },
};
