import { FooService, InjectedThing, cloesce, Env } from "./backend.js";
import { HttpResult } from "cloesce";

export class InjectedThingImpl extends InjectedThing {
  value: string = "injected value";
}

export const FooServiceImpl = FooService.impl({
  method(env) {
    const inj = env.InjectedThing as InjectedThingImpl;
    const injVal = inj.value;
    return HttpResult.ok(200, `foo's invocation; injected: ${injVal}`);
  },
});

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const app = await cloesce();
    app.register(new InjectedThingImpl());
    app.register(FooServiceImpl);

    return await app.run(request, env);
  },
};
