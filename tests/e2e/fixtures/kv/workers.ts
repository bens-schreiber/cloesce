import {
  createApp,
  ModelWithKv,
  KVOnly,
  KVSibling,
  AppConfig,
  KVOnlyWithSingleton,
  type Api,
  type CfEnv,
} from "./backend.js";

const modelWithKv: Api.ModelWithKv.Of = {
  acceptKvObject(item) {
    return item;
  },
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp()
      .worker(env)
      .register(ModelWithKv, modelWithKv)
      .register(KVOnly, {})
      .register(KVSibling, {})
      .register(AppConfig, {})
      .register(KVOnlyWithSingleton, {})
      .run(request);
  },
};
