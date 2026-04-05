import { HttpResult } from "cloesce";
import * as Cloesce from "./backend.js";

class InjectedThing extends Cloesce.InjectedThing {
    constructor(public readonly value: string) {
        super();
    }
}

class Foo extends Cloesce.Foo.Api {
    blockedMethod(): void {

    }
    getInjectedThing(thing: InjectedThing): string {
        return thing.value;
    }

}

export default {
    async fetch(request: Request, env: Cloesce.Env): Promise<Response> {
        if (request.method === "POST") {
            return HttpResult.fail(401, "POST methods aren't allowed.").toResponse();
        }

        const app = await Cloesce.cloesce();
        app.register(new Foo());

        app.onNamespace(Cloesce.Foo.Tag, (di) => {
            di.set(new InjectedThing("hello world"));
        })

        app.onMethod(Cloesce.Foo.Tag, "blockedMethod", (_di) => {
            return HttpResult.fail(401, "Blocked method");
        });

        const result = await app.run(request, env);
        result.headers.set("X-Cloesce-Test", "true");

        return result;
    }
}