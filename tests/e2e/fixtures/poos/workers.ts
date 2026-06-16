import { HttpResult } from "cloesce";
import { cloesce, CfEnv, PooAcceptYield } from "./backend.js";

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
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = cloesce(env);
    app.register(PooAcceptYieldImpl);
    return await app.run(request);
  },
};
