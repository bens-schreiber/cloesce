import { HttpResult } from "cloesce";
import { cloesce, Env, PooAcceptYield } from "./backend.js";

export const PooAcceptYieldImpl = PooAcceptYield.impl({
  acceptPoos() {
    return HttpResult.ok(200);
  },

  yieldPoo() {
    return HttpResult.ok(200, {
      a: { name: "name", major: "major" },
      b: [{ color: "color" }],
    });
  },
});

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const app = await cloesce();
    app.register(PooAcceptYieldImpl);
    return await app.run(request, env);
  },
};
