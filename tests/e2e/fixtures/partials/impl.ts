import { Dog, Env } from "./backend.ts";
import { HttpResult, Orm, DeepPartial } from "cloesce";

export class DogImpl extends Dog.Api {
    async create(dog: DeepPartial<Dog.Self>) {
        // env is injected at runtime via the backend
        throw new Error("env injection not available in impl stub");
    }

    getPartialSelf(self: Dog.Self) {
        return HttpResult.ok<DeepPartial<Dog.Self>>(200, { name: self.name });
    }
}
