import {
  Service,
  GET,
  CloesceApp,
  HttpResult,
} from "cloesce/backend";

class ExecutionContext { }

@Service
export class FooService {
  @GET
  static staticMethod(): string {
    return "foo's static invocation";
  }

  @GET
  instantiatedMethod(): string {
    return "foo's instantiated invocation";
  }
}

@Service
export class BarService {
  foo: FooService;

  @GET
  useFoo(): string {
    return `${this.foo.instantiatedMethod()} from BarService`;
  }
}

export default async function main(
  request: Request,
  env: any,
  app: CloesceApp,
  _ctx: ExecutionContext,
): Promise<Response> {
  app.onRun((di: Map<string, any>) => {
    if (!di.has(BarService.name)) {
      return HttpResult.fail(500, "Bar Service was not injected");
    }

    if (!di.has(FooService.name)) {
      return HttpResult.fail(500, "Foo Service was not injected");
    }
  });

  return await app.run(request, env);
}