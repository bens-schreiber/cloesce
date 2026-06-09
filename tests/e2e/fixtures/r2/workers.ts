import {
  D1BackedModel,
  R2Only,
  R2Sibling,
  Env,
  cloesce,
  CfReadableStream,
  bucket1,
} from "./backend.js";

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

export const R2OnlyImpl = R2Only.impl({
  async uploadData(self, env, data: CfReadableStream) {
    const key = bucket1.data(self.id);
    await env.bucket1.put(key, data as any);
  },
});

export const R2SiblingImpl = R2Sibling.impl({
  async uploadData(self, env, data: CfReadableStream) {
    const key = bucket1.otherData(self.siblingId);
    await env.bucket1.put(key, data as any);
  },
});

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const app = cloesce(env);
    app.register(D1BackedModelImpl);
    app.register(R2OnlyImpl);
    app.register(R2SiblingImpl);
    return await app.run(request);
  },
};
