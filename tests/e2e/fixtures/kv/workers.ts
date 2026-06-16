import { cloesce, CfEnv, ModelWithKv, KVOnly, KVSibling } from "./backend.js";

const ModelWithKvImpl = ModelWithKv.impl({
  acceptPaginated(ps) {
    return ps;
  },
});

const KVOnlyImpl = KVOnly.impl({});
const KVSiblingImpl = KVSibling.impl({});

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = cloesce(env);
    app.register(ModelWithKvImpl);
    app.register(KVOnlyImpl);
    app.register(KVSiblingImpl);
    return await app.run(request);
  },
};
