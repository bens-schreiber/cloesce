import {
  cloesce,
  CfEnv,
  ModelWithKv,
  KVOnly,
  KVSibling,
  AppConfig,
  KVOnlyWithSingleton,
} from "./backend.js";

const ModelWithKvImpl = ModelWithKv.impl({
  acceptPaginated(ps) {
    return ps;
  },
});

const KVOnlyImpl = KVOnly.impl({});
const KVSiblingImpl = KVSibling.impl({});
const AppConfigImpl = AppConfig.impl({});
const KVOnlyWithSingletonImpl = KVOnlyWithSingleton.impl({});

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = cloesce(env);
    app.register(
      ModelWithKvImpl,
      KVOnlyImpl,
      KVSiblingImpl,
      AppConfigImpl,
      KVOnlyWithSingletonImpl,
    );
    return await app.run(request);
  },
};
