import { describe, test, expect, vi, afterEach } from "vitest";
import {
  MatchedRoute,
  RouterError,
  RuntimeContainer,
  _cloesceInternal,
} from "../src/router/router";
import { HttpVerb, MediaType } from "../src/ast";
import { CloesceApp, HttpResult, DependencyContainer } from "../src/ui/backend";
import { ModelBuilder, ServiceBuilder, createAst } from "./builder";

function createRequest(url: string, method?: string, body?: any) {
  return new Request(url, {
    method,
    body: body && JSON.stringify(body),
  });
}

function createCtorReg(ctors?: (new () => any)[]) {
  const res: Record<string, new () => any> = {};
  if (ctors) {
    for (const ctor of ctors) {
      res[ctor.name] = ctor;
    }
  }

  return res;
}

function mockWranglerEnv() {
  return {
    db: {
      prepare: vi.fn(),
      exec: vi.fn(),
    } as any,
  };
}

function createDi() {
  return new DependencyContainer();
}

function extractErrorCode(str: string | undefined): number | null {
  const match = str?.match(/\(ErrorCode:\s*(\d+)\)/);
  return match ? Number(match[1]) : null;
}

describe("Match Route", () => {
  test("Unknown Prefix => 404", () => {
    // Arrange
    const request = createRequest("http://foo.com/does/not/match");
    const ast = createAst();

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, "api");

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(res.unwrapLeft().status).toEqual(404);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.UnknownPrefix,
    );
  });

  test("Unknown Route => 404", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/method");
    const ast = createAst();

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, "api");

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(res.unwrapLeft().status).toEqual(404);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.UnknownRoute,
    );
  });

  test("Unknown Method => 404", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/method");
    const ast = createAst({
      models: [ModelBuilder.model("Model").idPk().build()],
    });

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, "api");

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(res.unwrapLeft().status).toEqual(404);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.UnknownRoute,
    );
  });

  test("Unmatched Verb => 404", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/method");
    const ast = createAst({
      models: [
        ModelBuilder.model("Model")
          .idPk()
          .method("method", HttpVerb.Delete, false, [], "Void")
          .build(),
      ],
    });

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, "api");

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(res.unwrapLeft().status).toEqual(404);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.UnmatchedHttpVerb,
    );
  });

  test("Matches static method", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/method", "POST");
    const ast = createAst({
      models: [
        ModelBuilder.model("Model")
          .idPk()
          .method("method", HttpVerb.Post, true, [], "Void")
          .build(),
      ],
    });

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, "api");

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      primaryKey: null,
      keyParams: {},
      method: ast.models["Model"].methods["method"],
      model: ast.models["Model"],
      namespace: "Model",
      kind: "model",
    });
  });

  test("Matches instantiated method", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/0/method", "POST");
    const ast = createAst({
      models: [
        ModelBuilder.model("Model")
          .idPk()
          .method("method", HttpVerb.Post, false, [], "Void")
          .build(),
      ],
    });

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, "api");

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      primaryKey: "0",
      keyParams: {},
      model: ast.models["Model"],
      method: ast.models["Model"].methods["method"],
      namespace: "Model",
      kind: "model",
    });
  });

  test("Matches instantiated method with key params", () => {
    // Arrange
    const request = createRequest(
      "http://foo.com/api/Model/0/value1/value2/method",
      "POST",
    );
    const ast = createAst({
      models: [
        ModelBuilder.model("Model")
          .idPk()
          .method("method", HttpVerb.Post, false, [], "Void")
          .keyParam("key1")
          .keyParam("key2")
          .build(),
      ],
    });

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, "api");

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      primaryKey: "0",
      keyParams: {
        key1: "value1",
        key2: "value2",
      },
      model: ast.models["Model"],
      method: ast.models["Model"].methods["method"],
      namespace: "Model",
      kind: "model",
    });
  });

  test("Matches static method on Service", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Service/method", "POST");
    const ast = createAst({
      services: [
        ServiceBuilder.service("Service")
          .method("method", HttpVerb.Post, true, [], "Void")
          .build(),
      ],
    });

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, "api");

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      primaryKey: null,
      keyParams: {},
      method: ast.services["Service"].methods["method"],
      service: ast.services["Service"],
      kind: "service",
      namespace: "Service",
    });
  });

  test("Matches instantiated method on Service", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Service/method", "POST");
    const ast = createAst({
      services: [
        ServiceBuilder.service("Service")
          .method("method", HttpVerb.Post, false, [], "Void")
          .build(),
      ],
    });

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, "api");

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      primaryKey: null,
      keyParams: {},
      method: ast.services["Service"].methods["method"],
      service: ast.services["Service"],
      kind: "service",
      namespace: "Service",
    });
  });
});

describe("Namespace Middleware", () => {
  afterEach(() => {
    _cloesceInternal.RuntimeContainer.dispose();
  });

  test("Exits early on Model", async () => {
    // Arrange
    const env = mockWranglerEnv();
    const ast = createAst({
      models: [
        ModelBuilder.model("Foo")
          .idPk()
          .method("method", HttpVerb.Post, true, [], "Void")
          .build(),
      ],
    });
    const constructorRegistry = createCtorReg();
    class Foo {}
    constructorRegistry[Foo.name] = Foo;

    await RuntimeContainer.init(ast, constructorRegistry);
    const app = new CloesceApp();

    const request = createRequest("http://foo.com/api/Foo/method", "POST");
    const di = createDi();

    app.onNamespace(Foo, async () => {
      return HttpResult.fail(500, "oogly boogly");
    });

    // Act
    const res = await (app as any).router(
      request,
      env,
      ast,
      undefined,
      constructorRegistry,
      di,
    );

    // Assert
    expect(res.status).toBe(500);
    expect(res.message).toBe("oogly boogly");
  });

  test("Exits early on Service", async () => {
    // Arrange
    const env = mockWranglerEnv();
    const ast = createAst({
      services: [
        ServiceBuilder.service("Foo")
          .method("method", HttpVerb.Post, true, [], "Void")
          .build(),
      ],
    });
    const constructorRegistry = createCtorReg();
    class Foo {}
    constructorRegistry[Foo.name] = Foo;

    await RuntimeContainer.init(ast, constructorRegistry);
    const app = new CloesceApp();

    const request = createRequest("http://foo.com/api/Foo/method", "POST");
    const di = createDi();

    app.onNamespace(Foo, async () => {
      return HttpResult.fail(500, "oogly boogly");
    });

    // Act
    const res = await (app as any).router(
      request,
      env,
      ast,
      undefined,
      constructorRegistry,
      di,
    );

    // Assert
    expect(res.status).toBe(500);
    expect(res.message).toBe("oogly boogly");
  });
});

describe("Request Validation", () => {
  test("Instantiated model method missing id => 400", async () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Foo/method", "POST", {});
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("method", HttpVerb.Post, false, [], "Void")
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      model,
      method: model.methods["method"],
      primaryKey: null,
      keyParams: {},
    };

    const wasmMock = {} as any;
    const astMock = {} as any;
    const envMock = {} as any;
    const ctorRegMock = {} as any;

    // Act
    const res = await _cloesceInternal.validateRequest(
      request,
      wasmMock,
      astMock,
      envMock,
      ctorRegMock,
      route,
    );

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.InstantiatedMethodMissingPrimaryKey,
    );
  });

  test("Request Missing JSON Body => 400", async () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Foo/method", "POST");
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("method", HttpVerb.Post, true, [], "Void")
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.methods["method"],
      primaryKey: null,
      keyParams: {},
    };

    const wasmMock = {} as any;
    const astMock = {} as any;
    const envMock = {} as any;
    const ctorRegMock = {} as any;

    // Act
    const res = await _cloesceInternal.validateRequest(
      request,
      wasmMock,
      astMock,
      envMock,
      ctorRegMock,
      route,
    );

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.RequestMissingBody,
    );
  });
});

describe("Method Middleware", () => {
  afterEach(() => {
    _cloesceInternal.RuntimeContainer.dispose();
  });

  test("Exits early", async () => {
    // Arrange
    const env = mockWranglerEnv();
    const ast = createAst({
      models: [
        ModelBuilder.model("Foo")
          .idPk()
          .method("method", HttpVerb.Post, true, [], "Void")
          .build(),
      ],
    });
    const constructorRegistry = createCtorReg();
    class Foo {
      method() {}
    }
    constructorRegistry[Foo.name] = Foo;

    await RuntimeContainer.init(ast, constructorRegistry);
    const app = new CloesceApp();

    const request = createRequest(
      "http://foo.com/api/Foo/method",
      "POST",
      JSON.stringify({}),
    );

    const di = createDi();

    app.onMethod(Foo, "method", async () => {
      return HttpResult.fail(500, "oogly boogly");
    });

    // Act
    const res = await (app as any).router(
      request,
      env,
      ast,
      undefined,
      constructorRegistry,
      di,
    );

    // Assert
    expect(res.status).toBe(500);
    expect(res.message).toBe("oogly boogly");
  });
});

describe("Method Dispatch", () => {
  test("Void Return Type => 200, no data", async () => {
    // Arrange
    const crud = {
      testMethod() {
        return;
      },
    };

    const di = createDi();
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("testMethod", HttpVerb.Get, true, [], "Void")
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.methods["testMethod"],
      primaryKey: null,
      keyParams: {},
    };

    // Act
    const res = await _cloesceInternal.methodDispatch(crud, di, route, {});

    // Assert
    expect(res).toStrictEqual(HttpResult.ok(200).setMediaType(MediaType.Json));
    expect(res.data).toBeUndefined();
  });

  test("HttpResult Return Type => HttpResult", async () => {
    // Arrange
    const crud = {
      testMethod() {
        return HttpResult.ok(123, "foo");
      },
    };

    const di = createDi();

    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("testMethod", HttpVerb.Get, true, [], { HttpResult: "Void" })
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.methods["testMethod"],
      primaryKey: null,
      keyParams: {},
    };

    // Act
    const res = await _cloesceInternal.methodDispatch(crud, di, route, {});

    // Assert
    expect(res).toStrictEqual(
      HttpResult.ok(123, "foo").setMediaType(MediaType.Json),
    );
  });

  test("Primitive Return Type => HttpResult", async () => {
    // Arrange
    const crud: any = {
      testMethod() {
        return "neigh";
      },
    };
    const di = createDi();

    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("testMethod", HttpVerb.Get, true, [], "Text")
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.methods["testMethod"],
      primaryKey: null,
      keyParams: {},
    };

    // Act
    const res = await _cloesceInternal.methodDispatch(crud, di, route, {});

    // Assert
    expect(res).toStrictEqual(
      HttpResult.ok(200, "neigh").setMediaType(MediaType.Json),
    );
  });

  test("handles thrown errors", async () => {
    // Arrange – Error object
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("testMethod", HttpVerb.Get, true, [], "Text")
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.methods["testMethod"],
      primaryKey: null,
      keyParams: {},
    };

    const crud = {
      testMethod() {
        throw new Error("boom");
      },
    };

    const di = createDi();

    // Act
    const res = await _cloesceInternal.methodDispatch(crud, di, route, {});

    // Assert
    expect(extractErrorCode(res.message)).toBe(RouterError.UncaughtException);
    expect(res.status).toBe(500);
  });
});
