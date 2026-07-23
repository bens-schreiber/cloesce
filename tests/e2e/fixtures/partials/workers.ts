import { createApp, Worker, Dog, type Api, type CfEnv } from "./backend.js";

const dog: Api.Dog.Of = {
  create(env, model) {
    return env.db.dog.save(model);
  },

  getPartialSelf(self) {
    return { name: self.name };
  },
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker).register(Dog, dog).run(request);
  },
};
