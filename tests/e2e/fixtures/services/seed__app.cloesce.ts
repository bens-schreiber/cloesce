import { ExecutionContext } from "@cloudflare/workers-types";
import {
  Service,
  GET,
  CloesceApp,
  Inject,
  HttpResult,
  DependencyContainer,
} from "cloesce/backend";

const InjectedThingSymbol = Symbol.for("InjectedThing");
type InjectedThing = typeof InjectedThingSymbol;

@Service
export class FooService {
  @GET
  static staticMethod(@Inject thing: InjectedThing): string {
    if (!thing) {
      throw new Error("Injected thing is missing");
    }
    return "foo's static invocation";
  }

  @GET
  instantiatedMethod(@Inject thing: InjectedThing): string {
    if (!thing) {
      throw new Error("Injected thing is missing");
    }
    return "foo's instantiated invocation";
  }
}

@Service
export class BarService {
  foo: FooService;
  someCrap: string;

  async init(@Inject fooService: FooService): Promise<void> {
    if (!fooService) {
      throw new Error("FooService injection failed");
    }
    this.someCrap = "just some crap";
  }

  @GET
  useFoo(@Inject injectedThing: InjectedThing): string {
    if (!injectedThing) {
      throw new Error("Injected thing is missing");
    }
    return `${this.foo.instantiatedMethod(injectedThing)} from BarService, someCrap: ${this.someCrap}`;
  }
}

export default async function main(
  request: Request,
  env: any,
  app: CloesceApp,
  _ctx: ExecutionContext,
): Promise<Response> {
  app.onRoute((di) => {
    di.set(InjectedThingSymbol, "I am an injected thing");
  });

  app.onNamespace(FooService, (di: DependencyContainer) => {
    if (!di.has(BarService)) {
      return HttpResult.fail(500, "Bar Service was not injected");
    }

    if (!di.has(FooService)) {
      return HttpResult.fail(500, "Foo Service was not injected");
    }
  });

  app.onNamespace(BarService, (di) => {
    if (!di.has(BarService)) {
      return HttpResult.fail(500, "Bar Service was not injected");
    }

    if (!di.has(FooService)) {
      return HttpResult.fail(500, "Foo Service was not injected");
    }
  });

  return await app.run(request, env);
}
