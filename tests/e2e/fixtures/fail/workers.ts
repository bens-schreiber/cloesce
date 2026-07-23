import { createApp, Worker, FailModel, type Api, type CfEnv } from "./backend.js";

const failModel: Api.FailModel.Of = {
  throwingMethod() {
    throw new Error("intentional failure from throwingMethod");
  },

  numericValidators() {},

  stringValidators() {},
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    // NOTE: UnregisteredService is intentionally NOT registered, to exercise the
    // NotImplemented (501) router branch
    const app = createApp(env, Worker).register(FailModel, failModel);

    // @ts-expect-error
    return (app as { run(request: Request): Promise<Response> }).run(request);
  },
};
