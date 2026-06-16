import { FooService, InjectedThing, cloesce, CfEnv } from "./backend.js";
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
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = cloesce(env);
    app.register(new InjectedThingImpl(), FooServiceImpl);

    return await app.run(request);
  },
};
