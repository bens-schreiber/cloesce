import { FooService, BarService } from "./backend.ts";
import { HttpResult } from "cloesce";

interface InjectedThing {
    value: string;
}

export class FooServiceImpl extends FooService.Api {
    init(self: FooService.Self) {
        // nothing to initialize
    }

    staticMethod(thing: InjectedThing) {
        if (!thing) throw new Error("Injected thing is missing");
        return HttpResult.ok(200, "foo's static invocation");
    }

    instantiatedMethod(self: FooService.Self, thing: InjectedThing) {
        if (!thing) throw new Error("Injected thing is missing");
        return HttpResult.ok(200, "foo's instantiated invocation");
    }
}

export class BarServiceImpl extends BarService.Api {
    async init(self: BarService.Self) {
        if (!self.foo) throw new Error("FooService injection failed");
    }

    useFoo(self: BarService.Self, injectedThing: InjectedThing) {
        if (!injectedThing) throw new Error("Injected thing is missing");
        return HttpResult.ok(200, `foo's instantiated invocation from BarService, someCrap: just some crap`);
    }
}
