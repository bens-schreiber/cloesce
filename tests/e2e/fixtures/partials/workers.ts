import { cloesce, Dog, Env } from "./backend.js";
import { HttpResult, Orm, DeepPartial } from "cloesce";

export class DogImpl extends Dog.Api {
    async create(env: Env, dog: DeepPartial<Dog.Self>): Promise<Dog.Self> {
        return (await Dog.save(env, dog))!
    }


    getPartialSelf(self: Dog.Self) {
        return HttpResult.ok<DeepPartial<Dog.Self>>(200, { name: self.name });
    }
}


export default {
    async fetch(request: Request, env: Env): Promise<Response> {
        const app = await cloesce();
        app.register(new DogImpl());
        return await app.run(request, env);
    }
}