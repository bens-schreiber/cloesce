import { HttpResult } from "cloesce";
import { createApp, Worker, Validator, type Api, type CfEnv } from "./backend.js";

const validator: Api.Validator.Of = {
  someMethod(_self, id, name) {
    if (id >= 100) {
      return HttpResult.fail(500, "ID must be less than 100");
    }

    if (name.length != 10) {
      return HttpResult.fail(500, "Name must be exactly 10 characters long");
    }

    return HttpResult.ok(200);
  },
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker).register(Validator, validator).run(request);
  },
};
