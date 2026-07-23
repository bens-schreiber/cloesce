import { HttpResult } from "cloesce";
import { createApp, Worker, PooAcceptYield, type Api, type CfEnv } from "./backend.js";

const pooAcceptYield: Api.PooAcceptYield.Of = {
  acceptPoos() {
    return HttpResult.ok(200);
  },

  yieldPoo() {
    return {
      a: { name: "name", major: "major" },
      b: [{ color: "color" }],
    };
  },
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker).register(PooAcceptYield, pooAcceptYield).run(request);
  },
};
