import { PureR2Model, D1BackedModel, Env, cloesce, CfReadableStream } from "./backend.js";

export const PureR2ModelImpl = PureR2Model.impl({
  async uploadData(self, env, data: CfReadableStream) {
    const key = this.Key.data(self.id);
    await env.bucket1.put(key, data);
  },

  async uploadOtherData(self, env, data: CfReadableStream) {
    const key = this.Key.otherData(self.id);
    await env.bucket1.put(key, data as any);
  },
});

export const D1BackedModelImpl = D1BackedModel.impl({
  async uploadData(self, e, data: CfReadableStream) {
    const key = this.Key.r2Data(self.id, self.keyParam, self.someColumn, self.someOtherColumn);
    await e.bucket1.put(key, data as any);
  },
});

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const app = await cloesce();
    app.register(PureR2ModelImpl);
    app.register(D1BackedModelImpl);
    return await app.run(request, env);
  },
};
