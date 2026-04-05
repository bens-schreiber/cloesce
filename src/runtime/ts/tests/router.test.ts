import { describe, test, expect, vi, afterEach } from "vitest";
import {
  MatchedRoute,
  RouterError,
  RuntimeContainer,
  _cloesceInternal,
} from "../src/router/router";
import { CloesceApp, HttpResult, DependencyContainer } from "../src/ui/backend";
import { ModelBuilder, ServiceBuilder, createAst } from "./builder";
import { Model, Service } from "../src/cidl";

function createRequest(url: string, method?: string, body?: any) {
  return new Request(url, {
    method,
    body: body && JSON.stringify(body),
  });
}

const mockImpl = vi.fn();

function createRegistry(...namespaces: (Model | Service)[]) {
  const map = new Map<string, any>();
  for (const ns of namespaces) {
    const methodMap: Record<string, any> = {};
    for (const method of ns.apis) {
      methodMap[method.name] = mockImpl;
    }
    map.set(ns.name, methodMap);
  }
  return map;
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

const api = "http://foo.com/api";

describe("Match Route", () => {
  test("Unknown Prefix => 404", () => {
    // Arrange
    const request = createRequest("http://foo.com/does/not/match");
    const ast = createAst();
    const env = mockWranglerEnv();
    const registry = createRegistry();

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, api, registry, env);

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
    const env = mockWranglerEnv();
    const registry = createRegistry();

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, api, registry, env);

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
    const env = mockWranglerEnv();
    const registry = createRegistry();

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, api, registry, env);

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
          .method("method", "Delete", [], "Void")
          .build(),
      ],
    });
    const env = mockWranglerEnv();
    const registry = createRegistry();

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, api, registry, env);

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
          .method("method", "Post", [], "Void")
          .build(),
      ],
    });
    const env = mockWranglerEnv();
    const registry = createRegistry(ast.models["Model"]);

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, api, registry, env);

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      getParamValues: {},
      keyFields: {},
      method: ast.models["Model"].apis.find((m) => m.name === "method"),
      model: ast.models["Model"],
      namespace: "Model",
      kind: "model",
      impl: mockImpl,
    });
  });

  test("Matches instantiated method", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/0/method", "POST");
    const ast = createAst({
      models: [
        ModelBuilder.model("Model")
          .idPk()
          .method("method", "Post", [], "Void", "ds")
          .dataSource("ds", {}, [{ name: "id", cidl_type: "Integer" }])
          .build(),
      ],
    });
    const env = mockWranglerEnv();
    const registry = createRegistry(ast.models["Model"]);

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, api, registry, env);

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      dataSource: ast.models["Model"].data_sources["ds"],
      getParamValues: { id: "0" },
      impl: mockImpl,
      keyFields: {},
      model: ast.models["Model"],
      method: ast.models["Model"].apis.find((m) => m.name === "method"),
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
          .method("method", "Post", [], "Void", "ds")
          .dataSource("ds", {}, [{ name: "id", cidl_type: "Integer" }])
          .keyField("key1")
          .keyField("key2")
          .build(),
      ],
    });
    const env = mockWranglerEnv();
    const registry = createRegistry(ast.models["Model"]);

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, api, registry, env);

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      dataSource: ast.models["Model"].data_sources["ds"],
      impl: mockImpl,
      getParamValues: { id: "0" },
      keyFields: {
        key1: "value1",
        key2: "value2",
      },
      model: ast.models["Model"],
      method: ast.models["Model"].apis.find((m) => m.name === "method"),
      namespace: "Model",
      kind: "model",
    });
  });

  test("Matches instantiated method with composite primary key and key fields", () => {
    // Arrange
    const request = createRequest(
      "http://foo.com/api/Model/acme/user123/value1/value2/method",
      "POST",
    );
    const ast = createAst({
      models: [
        ModelBuilder.model("Model")
          .pk("orgId", "String")
          .pk("userId", "String")
          .method("method", "Post", [], "Void", "ds")
          .dataSource("ds", {}, [
            { name: "orgId", cidl_type: "String" },
            { name: "userId", cidl_type: "String" },
          ])
          .keyField("key1")
          .keyField("key2")
          .build(),
      ],
    });
    const env = mockWranglerEnv();
    const registry = createRegistry(ast.models["Model"]);

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, api, registry, env);

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      dataSource: ast.models["Model"].data_sources["ds"],
      impl: mockImpl,
      getParamValues: { orgId: "acme", userId: "user123" },
      keyFields: {
        key1: "value1",
        key2: "value2",
      },
      model: ast.models["Model"],
      method: ast.models["Model"].apis.find((m) => m.name === "method"),
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
          .method("method", "Post", true, [], "Void")
          .build(),
      ],
    });
    const env = mockWranglerEnv();
    const registry = createRegistry(ast.services["Service"]);

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, api, registry, env);

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      getParamValues: {},
      impl: mockImpl,
      keyFields: {},
      method: ast.services["Service"].apis.find((m) => m.name === "method"),
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
          .method("method", "Post", false, [], "Void")
          .build(),
      ],
    });
    const env = mockWranglerEnv();
    const registry = createRegistry(ast.services["Service"]);

    // Act
    const res = _cloesceInternal.matchRoute(request, ast, api, registry, env);

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap()).toEqual({
      getParamValues: {},
      impl: mockImpl,
      keyFields: {},
      method: ast.services["Service"].apis.find((m) => m.name === "method"),
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

  test("Exits early", async () => {
    // Arrange
    const env = mockWranglerEnv();
    const ast = createAst({
      models: [
        ModelBuilder.model("Foo")
          .idPk()
          .method("method", "Post", [], "Void")
          .build(),
      ],
    });

    await RuntimeContainer.init(ast, api);
    const app = new CloesceApp();
    app.register({
      tag: "Foo",
      method: mockImpl,
    } as any);

    const request = createRequest("http://foo.com/api/Foo/method", "POST");
    const di = createDi();

    app.onNamespace("Foo", async () => {
      return HttpResult.fail(500, "oogly boogly");
    });

    // Act
    const res = await (app as any).router(
      request,
      env,
      ast,
      undefined,
      di,
      api,
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
      .method("method", "Post", [], "Void", "ds")
      .dataSource("ds", {}, [{ name: "id", cidl_type: "Integer" }])
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      model,
      method: model.apis.find((m) => m.name === "method")!,
      getParamValues: {},
      keyFields: {},
      dataSource: model.data_sources["ds"],
      impl: mockImpl,
    };

    const wasmMock = {} as any;
    const astMock = {} as any;
    const envMock = {} as any;

    // Act
    const res = await _cloesceInternal.validateRequest(
      request,
      wasmMock,
      astMock,
      envMock,
      route,
    );

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.InstantiatedMethodMissingGetParam,
    );
  });

  test("Request Missing JSON Body => 400", async () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Foo/method", "POST");
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("method", "Post", [], "Void")
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "method")!,
      getParamValues: {},
      keyFields: {},
      impl: mockImpl,
    };

    const wasmMock = {} as any;
    const astMock = {} as any;
    const envMock = {} as any;

    // Act
    const res = await _cloesceInternal.validateRequest(
      request,
      wasmMock,
      astMock,
      envMock,
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
          .method("method", "Post", [], "Void")
          .build(),
      ],
    });

    await RuntimeContainer.init(ast, api);
    const app = new CloesceApp();
    app.register({
      tag: "Foo",
      method: mockImpl,
    } as any);

    const request = createRequest(
      "http://foo.com/api/Foo/method",
      "POST",
      JSON.stringify({}),
    );

    const di = createDi();

    app.onMethod("Foo", "method", async () => {
      return HttpResult.fail(500, "oogly boogly");
    });

    // Act
    const res = await (app as any).router(
      request,
      env,
      ast,
      undefined,
      di,
      api,
    );

    // Assert
    expect(res.status).toBe(500);
    expect(res.message).toBe("oogly boogly");
  });
});

describe("Method Dispatch", () => {
  test("Void Return Type => 200, no data", async () => {
    // Arrange

    const di = createDi();
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("testMethod", "Get", [], "Void")
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "testMethod")!,
      impl: () => {},
      getParamValues: {},
      keyFields: {},
    };

    // Act
    const res = await _cloesceInternal.methodDispatch({}, di, route, {});

    // Assert
    expect(res).toStrictEqual(HttpResult.ok(200).setMediaType("Json"));
    expect(res.data).toBeUndefined();
  });

  test("HttpResult Return Type => HttpResult", async () => {
    // Arrange
    const di = createDi();

    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("testMethod", "Get", [], { HttpResult: "Void" })
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "testMethod")!,
      impl: () => HttpResult.ok(123, "foo"),
      getParamValues: {},
      keyFields: {},
    };

    // Act
    const res = await _cloesceInternal.methodDispatch({}, di, route, {});

    // Assert
    expect(res).toStrictEqual(HttpResult.ok(123, "foo").setMediaType("Json"));
  });

  test("Non HttpResult => HttpResult", async () => {
    // Arrange
    const di = createDi();

    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("testMethod", "Get", [], "String")
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "testMethod")!,
      impl: () => "neigh",
      getParamValues: {},
      keyFields: {},
    };

    // Act
    const res = await _cloesceInternal.methodDispatch({}, di, route, {});

    // Assert
    expect(res).toStrictEqual(HttpResult.ok(200, "neigh").setMediaType("Json"));
  });

  test("handles thrown errors", async () => {
    // Arrange
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("testMethod", "Get", [], "String")
      .build();

    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "testMethod")!,
      impl: () => {
        throw new Error("boom");
      },
      getParamValues: {},
      keyFields: {},
    };

    const di = createDi();

    // Act
    const res = await _cloesceInternal.methodDispatch({}, di, route, {});

    // Assert
    expect(extractErrorCode(res.message)).toBe(RouterError.UncaughtException);
    expect(res.status).toBe(500);
  });
});
