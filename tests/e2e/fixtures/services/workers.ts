import { FooService, BarService, InjectedThing, cloesce, Env } from "./backend.js";
import { HttpResult } from "cloesce";

export class InjectedThingImpl extends InjectedThing {
    value: string = "injected value";
}

export class FooServiceImpl extends FooService.Api {
    init(self: FooService.Self) { }

    staticMethod(thing: InjectedThing): string {
        return "foo's static invocation"
    }

    instantiatedMethod(self: FooService.Self, thing: InjectedThing): string {
        return `foo's instantiated invocation`;
    }
}

export class BarServiceImpl extends BarService.Api {
    async init(self: BarService.Self) {
        if (!self.foo) throw new Error("FooService injection failed");
    }

    useFoo(self: BarService.Self, injectedThing: InjectedThingImpl) {
        if (!injectedThing) throw new Error("Injected thing is missing");
        return HttpResult.ok(200, `foo's instantiated invocation from BarService; injected: ${injectedThing.value}`);
    }
}


export default {
    async fetch(request: Request, env: Env): Promise<Response> {
        const app = await cloesce();
        app.register(new InjectedThingImpl());
        app.register(new FooServiceImpl());
        app.register(new BarServiceImpl());

        return await app.run(request, env);
    }
}