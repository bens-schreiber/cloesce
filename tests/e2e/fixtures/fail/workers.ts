import * as clo from "./backend.js";

export const FailModel = clo.FailModel.impl({
  throwingMethod() {
    throw new Error("intentional failure from throwingMethod");
  },

  numericValidators() {},

  stringValidators() {},
});

export default {
  async fetch(request: Request, env: clo.Env): Promise<Response> {
    const app = await clo.cloesce();
    app.register(FailModel);
    // NOTE: UnregisteredService is intentionally NOT registered, to exercise
    // the NotImplemented (501) router branch.
    return await app.run(request, env);
  },
};
