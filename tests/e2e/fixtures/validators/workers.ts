import { HttpResult } from "cloesce";
import * as clo from "./backend.js";

export const Validator = clo.Validator.impl({
  someMethod(self, id, name): HttpResult<void> {
    if (id >= 100) {
      return HttpResult.fail(500, "ID must be less than 100");
    }

    if (name.length != 10) {
      return HttpResult.fail(500, "Name must be exactly 10 characters long");
    }

    return HttpResult.ok(200);
  },
});

export default {
  async fetch(request: Request, env: clo.Env): Promise<Response> {
    const app = await clo.cloesce();
    app.register(Validator);
    return await app.run(request, env);
  },
};
