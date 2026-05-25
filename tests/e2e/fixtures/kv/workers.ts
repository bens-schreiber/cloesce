import { cloesce, Env, ModelWithKv } from "./backend.js";

const ModelWithKvImpl = ModelWithKv.impl({
  acceptPaginated(ps) {
    return ps;
  },
});

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const app = await cloesce();
    app.register(ModelWithKvImpl);
    return await app.run(request, env);
  },
};
