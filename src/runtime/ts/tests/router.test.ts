import { describe, test, expect, vi } from "vitest";
import { MatchedRoute, _cloesceInternal } from "../src/router/router";
import { HttpResult } from "../src/ui/backend";
import { ModelBuilder, createIdl } from "./builder";

const { RouterError } = _cloesceInternal;

function createRequest(url: string, method?: string, body?: any) {
  return new Request(url, {
    method,
    body: body && JSON.stringify(body),
  });
}

const mockImpl = vi.fn();

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

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api);

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(res.unwrapLeft().status).toEqual(404);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(RouterError.UnknownPrefix);
  });

  test("Unknown Route => 404", () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Model/method");
    const idl = createIdl();

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api);

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

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api);

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

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api);

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

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api);

    // Assert
    expect(res.unwrap()).toEqual({
      getParamValues: {},
      method: idl.models["Model"].apis.find((m) => m.name === "method"),
      model: idl.models["Model"],
      namespace: "Model",
      forward: false,
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

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api);

    // Assert
    expect(res.unwrap()).toEqual({
      dataSource: idl.models["Model"].data_sources["ds"],
      getParamValues: { id: "0" },
      forward: false,
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

    // Act
    const res = _cloesceInternal.matchRoute(request, idl, api);

    // Assert
    expect(res.unwrap()).toEqual({
      dataSource: idl.models["Model"].data_sources["ds"],
      forward: false,
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
      forward: false,
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
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method("method", "Post", [{ name: "payload", cidl_type: "String", source: "Body" }], "Void")
      .build();

    const route: MatchedRoute = {
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "method")!,
      getParamValues: {},
      impl: mockImpl,
      model,
      forward: false,
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

  test("Missing required header param => 400", async () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Foo/method", "POST", {});
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method(
        "method",
        "Post",
        [{ name: "X_Tenant", cidl_type: "String", source: "Header" }],
        "Void",
      )
      .build();

    const route: MatchedRoute = {
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "method")!,
      getParamValues: {},
      impl: mockImpl,
      model,
      forward: false,
    };

    const wasmMock = {} as any;
    const idlMock = {} as any;
    const envMock = {} as any;

    // Act
    const res = await _cloesceInternal.validateRequest(request, wasmMock, idlMock, envMock, route);

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.RequestBodyMissingParameters,
    );
  });
});

describe("Method Dispatch", () => {
  test("Void Return Type => 200, no data", async () => {
    // Arrange
    const model = ModelBuilder.model("Foo").idPk().method("testMethod", "Get", [], "Void").build();

    const route: MatchedRoute = {
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "testMethod")!,
      impl: () => {},
      getParamValues: {},
      model,
      forward: false,
    };

    // Act
    const res = await _cloesceInternal.methodDispatch(route.impl!, {}, route, {}, {}, undefined);

    // Assert
    expect(res).toStrictEqual(HttpResult.ok(200).setMediaType("Json"));
    expect(res.data).toBeUndefined();
  });

  test("Directly returns HttpResult", async () => {
    // Arrange
    const model = ModelBuilder.model("Foo").idPk().method("testMethod", "Get", [], "Void").build();

    const route: MatchedRoute = {
      namespace: "Foo",
      method: model.apis.find((m) => m.name === "testMethod")!,
      impl: () => HttpResult.ok(123, "foo"),
      getParamValues: {},
      model,
      forward: false,
    };

    // Act
    const res = await _cloesceInternal.methodDispatch(route.impl!, {}, route, {}, {}, undefined);

    // Assert
    expect(res).toStrictEqual(HttpResult.ok(123, "foo").setMediaType("Json"));
  });

  test("Indirectly returns HttpResult", async () => {
    // Arrange
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
      forward: false,
    };

    // Act
    const res = await _cloesceInternal.methodDispatch(route.impl!, {}, route, {}, {}, undefined);

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
      forward: false,
    };

    // Act
    const res = await _cloesceInternal.methodDispatch(route.impl!, {}, route, {}, {}, undefined);

    // Assert
    expect(extractErrorCode(res.message)).toBe(RouterError.UncaughtException);
    expect(res.status).toBe(500);
  });

  test("passes the full env to an injecting route", async () => {
    // Arrange
    const env = { DB_1: { prepare: vi.fn() }, YouTubeApi: { key: "secret" } };

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
      forward: false,
    };

    // Act
    const res = await _cloesceInternal.methodDispatch(
      impl,
      {},
      route,
      { name: "ben" },
      env,
      undefined,
    );

    // Assert
    expect(res.status).toBe(200);
    expect(impl).toHaveBeenCalledWith(env, "ben");
  });
});

describe("Forwarding", () => {
  function createDurableIdl() {
    const method = {
      ...ModelBuilder.model("Leaderboard")
        .method("topScores", "Get", [], "Json")
        .build()
        .apis.find((m) => m.name === "topScores")!,
      durable_target: { binding: "LeaderboardDo", shard_args: ["tenantId"] },
    };
    const model = ModelBuilder.model("Leaderboard").build();
    model.apis = [method];
    return createIdl({ models: [model] });
  }

  function createRequest(forwarded: boolean) {
    const headers = forwarded ? { "cloesce-forwarded": "true" } : undefined;
    return new Request("http://foo.com/api/Leaderboard/topScores", { method: "GET", headers });
  }

  test("no local impl is marked for forwarding", () => {
    // Arrange
    const idl = createDurableIdl();

    // Act
    const res = _cloesceInternal.matchRoute(createRequest(false), idl, api);

    // Assert
    expect(res.isLeft()).toBe(false);
    expect(res.unwrap().forward).toBe(true);
  });

  test("an already-forwarded request is not forwarded again", () => {
    // Arrange
    const idl = createDurableIdl();

    // Act
    const res = _cloesceInternal.matchRoute(createRequest(true), idl, api);

    // Assert
    expect(res.isLeft()).toBe(false);
    expect(res.unwrap().forward).toBe(false);
  });
});
