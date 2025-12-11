import {
  Service,
  GET,
  CloesceApp,
  HttpResult,
} from "cloesce/backend";

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

const app: CloesceApp = new CloesceApp();

app.onRequest((request: Request, env, di: Map<string, any>) => {
  if (!di.has(BarService.name)) {
    return HttpResult.fail(500, "Bar Service was not injected");
  }

  if (!di.has(FooService.name)) {
    return HttpResult.fail(500, "Foo Service was not injected");
  }
});

export default app;
