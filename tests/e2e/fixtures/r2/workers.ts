import {
  createApp,
  Worker,
  D1BackedModel,
  R2Only,
  R2Sibling,
  type Api,
  type CfEnv,
} from "./backend.js";

const d1BackedModel: Api.D1BackedModel.Of = {
  async uploadData(self, env, data) {
    await env.bucket1.data.put(self.id, data);
  },

  async uploadOtherData(self, env, data) {
    await env.bucket1.otherData.put(self.id, data);
  },
};

const r2Only: Api.R2Only.Of = {
  async uploadData(self, env, data) {
    await env.bucket1.data.put(self.id, data);
  },
};

const r2Sibling: Api.R2Sibling.Of = {
  async uploadData(self, env, data) {
    await env.bucket1.otherData.put(self.siblingId, data);
  },
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker)
      .register(D1BackedModel, d1BackedModel)
      .register(R2Only, r2Only)
      .register(R2Sibling, r2Sibling)
      .run(request);
  },
};
