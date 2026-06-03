import { describe, test, expect, vi } from "vitest";
import { MatchedRoute, RouterError, _cloesceInternal } from "../src/router/router";
import { HttpResult, DependencyContainer } from "../src/ui/backend";
import { ModelBuilder, createIdl } from "./builder";
import { Model } from "../src/cidl";

function createRequest(url: string, method?: string, body?: any) {
  return new Request(url, {
    method,
    body: body && JSON.stringify(body),
  });
}

const mockImpl = vi.fn();

function createRegistry(...namespaces: Model[]) {
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
    const idl = createIdl();
    const registry = createRegistry();

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api, registry);

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(res.unwrapLeft().status).toEqual(404);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(RouterError.UnknownPrefix);
  });

  test("Unknown Route => 404", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/method");
    const idl = createIdl();
    const registry = createRegistry();

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api, registry);

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(res.unwrapLeft().status).toEqual(404);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(RouterError.UnknownRoute);
  });

  test("Unknown Method => 404", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/method");
    const idl = createIdl({
      models: [ModelBuilder.model("Model").idPk().build()],
    });
    const registry = createRegistry();

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api, registry);

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(res.unwrapLeft().status).toEqual(404);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(RouterError.UnknownRoute);
  });

  test("Unmatched Verb => 404", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/method");
    const idl = createIdl({
      models: [ModelBuilder.model("Model").idPk().method("method", "Delete", [], "Void").build()],
    });
    const registry = createRegistry();

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api, registry);

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(res.unwrapLeft().status).toEqual(404);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(RouterError.UnmatchedHttpVerb);
  });

  test("Matches static method", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/method", "POST");
    const idl = createIdl({
      models: [ModelBuilder.model("Model").idPk().method("method", "Post", [], "Void").build()],
    });
    const registry = createRegistry(idl.models["Model"]);

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api, registry);

    // Assert
    expect(res.unwrap()).toEqual({
      getParamValues: {},
      method: idl.models["Model"].apis.find((m) => m.name === "method"),
      model: idl.models["Model"],
      namespace: "Model",
      impl: mockImpl,
    });
  });

  test("Matches instantiated method", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/0/method", "POST");
    const idl = createIdl({
      models: [
        ModelBuilder.model("Model")
          .idPk()
          .method("method", "Post", [], "Void", "ds")
          .dataSource("ds", {}, [{ name: "id", cidl_type: "Int" }])
          .build(),
      ],
    });
    const registry = createRegistry(idl.models["Model"]);

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api, registry);

    // Assert
    expect(res.unwrap()).toEqual({
      dataSource: idl.models["Model"].data_sources["ds"],
      getParamValues: { id: "0" },
      impl: mockImpl,
      model: idl.models["Model"],
      method: idl.models["Model"].apis.find((m) => m.name === "method"),
      namespace: "Model",
    });
  });

  test("Matches instantiated method with composite primary key", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/acme/user123/method", "POST");
    const idl = createIdl({
      models: [
        ModelBuilder.model("Model")
          .pk("orgId", "String")
          .pk("userId", "String")
          .method("method", "Post", [], "Void", "ds")
          .dataSource("ds", {}, [
            { name: "orgId", cidl_type: "String" },
            { name: "userId", cidl_type: "String" },
          ])
          .build(),
      ],
    });
    const registry = createRegistry(idl.models["Model"]);

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api, registry);

    // Assert
    expect(res.unwrap()).toEqual({
      dataSource: idl.models["Model"].data_sources["ds"],
      impl: mockImpl,
      getParamValues: { orgId: "acme", userId: "user123" },
      model: idl.models["Model"],
      method: idl.models["Model"].apis.find((m) => m.name === "method"),
      namespace: "Model",
    });
  });
});

describe("Request Validation", () => {
  test("Instantiated model method missing id => 400", async () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Foo/method", "POST", {});
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("method", "Post", [], "Void", "ds")
      .dataSource("ds", {}, [{ name: "id", cidl_type: "Int" }])
      .build();

    const route: MatchedRoute = {
      namespace: "Foo",
      model,
      method: model.apis.find((m) => m.name === "method")!,
      getParamValues: {},
      dataSource: model.data_sources["ds"],
      impl: mockImpl,
    };

    const wasmMock = {} as any;
    const idlMock = {} as any;
    const envMock = {} as any;

    // Act
    const res = await _cloesceInternal.validateRequest(request, wasmMock, idlMock, envMock, route);

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.InstantiatedMethodMissingGetParam,
    );
  });

  test("Request Missing JSON Body => 400", async () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Foo/method", "POST");
    const model = ModelBuilder.model("Foo").idPk().method("method", "Post", [], "Void").build();

    const route: MatchedRoute = {
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "method")!,
      getParamValues: {},
      impl: mockImpl,
      model,
    };

    const wasmMock = {} as any;
    const idlMock = {} as any;
    const envMock = {} as any;

    // Act
    const res = await _cloesceInternal.validateRequest(request, wasmMock, idlMock, envMock, route);

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(RouterError.RequestMissingBody);
  });
});

describe("Method Dispatch", () => {
  test("Void Return Type => 200, no data", async () => {
    // Arrange

    const di = createDi();
    const model = ModelBuilder.model("Foo").idPk().method("testMethod", "Get", [], "Void").build();

    const route: MatchedRoute = {
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "testMethod")!,
      impl: () => {},
      getParamValues: {},
      model,
    };

    // Act
    const res = await _cloesceInternal.methodDispatch({}, di, route, {}, {});

    // Assert
    expect(res).toStrictEqual(HttpResult.ok(200).setMediaType("Json"));
    expect(res.data).toBeUndefined();
  });

  test("Directly returns HttpResult", async () => {
    // Arrange
    const di = createDi();

    const model = ModelBuilder.model("Foo").idPk().method("testMethod", "Get", [], "Void").build();

    const route: MatchedRoute = {
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "testMethod")!,
      impl: () => HttpResult.ok(123, "foo"),
      getParamValues: {},
      model,
    };

    // Act
    const res = await _cloesceInternal.methodDispatch({}, di, route, {}, {});

    // Assert
    expect(res).toStrictEqual(HttpResult.ok(123, "foo").setMediaType("Json"));
  });

  test("Indirectly returns HttpResult", async () => {
    // Arrange
    const di = createDi();

    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("testMethod", "Get", [], "String")
      .build();

    const route: MatchedRoute = {
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "testMethod")!,
      impl: () => "neigh",
      getParamValues: {},
      model,
    };

    // Act
    const res = await _cloesceInternal.methodDispatch({}, di, route, {}, {});

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
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "testMethod")!,
      impl: () => {
        throw new Error("boom");
      },
      getParamValues: {},
      model,
    };

    const di = createDi();

    // Act
    const res = await _cloesceInternal.methodDispatch({}, di, route, {}, {});

    // Assert
    expect(extractErrorCode(res.message)).toBe(RouterError.UncaughtException);
    expect(res.status).toBe(500);
  });

  test("passes bundled explicit injected values", async () => {
    // Arrange
    const di = createDi();
    const injectedObject = { tag: "YouTubeApi", key: "secret" };
    const db = { prepare: vi.fn() };
    di.set({ tag: "YouTubeApi" }, injectedObject);

    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("testMethod", "Post", [{ name: "name", cidl_type: "String" }], "Void")
      .build();

    const impl = vi.fn(() => undefined);
    const route: MatchedRoute = {
      namespace: "Foo",
      method: {
        ...model.apis.find((m) => m.name === "testMethod")!,
        injected: ["DB_1", "YouTubeApi"],
      },
      impl,
      getParamValues: {},
      model,
    };

    // Act
    const res = await _cloesceInternal.methodDispatch({}, di, route, { name: "ben" }, { DB_1: db });

    // Assert
    expect(res.status).toBe(200);
    expect(impl).toHaveBeenCalledWith({ DB_1: db, YouTubeApi: injectedObject }, "ben");
  });
});
