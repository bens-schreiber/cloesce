import { cloesce, Weather, Env } from "./backend.js";

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const app = await cloesce();
    app.register(Weather.impl({}));
    return await app.run(request, env);
  },
};
