import { cloesce, Dog, Env } from "./backend.js";
import { HttpResult, DeepPartial } from "cloesce";

export const DogImpl = Dog.impl({
    async create(env: Env, dog: DeepPartial<Dog.Self>): Promise<Dog.Self> {
        return (await this.Orm.save(env, dog))!
    },


    getPartialSelf(self: Dog.Self) {
        return HttpResult.ok<DeepPartial<Dog.Self>>(200, { name: self.name });
    },
});


export default {
    async fetch(request: Request, env: Env): Promise<Response> {
        const app = await cloesce();
        app.register(DogImpl);
        return await app.run(request, env);
    }
}