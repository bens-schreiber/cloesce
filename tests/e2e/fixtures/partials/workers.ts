import { cloesce, Dog, CfEnv } from "./backend.js";
import { HttpResult, DeepPartial } from "cloesce";

export const DogImpl = Dog.impl({
  async create(env: CfEnv, dog: DeepPartial<Dog.Self>): Promise<Dog.Self> {
    return (await this.Orm.save(env, dog)).value!;
  },

  getPartialSelf(self: Dog.Self) {
    return HttpResult.ok<DeepPartial<Dog.Self>>(200, { name: self.name });
  },
});

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = cloesce(env);
    app.register(DogImpl);
    return await app.run(request);
  },
};
