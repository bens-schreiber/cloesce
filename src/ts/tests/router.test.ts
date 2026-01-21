import { describe, test, expect, vi, beforeAll } from "vitest";
import {
  MatchedRoute,
  RouterError,
  _cloesceInternal,
} from "../src/router/router";
import { HttpVerb, MediaType, Model, NamedTypedValue } from "../src/ast";
import { CloesceApp, HttpResult } from "../src/ui/backend";
import { ModelBuilder, ServiceBuilder, createAst } from "./builder";

function createRequest(url: string, method?: string, body?: any) {
  return new Request(url, {
    method,
    body: body && JSON.stringify(body),
  });
}

function createCtorReg(ctors?: (new () => any)[]) {
  const res = {};
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
  return new Map<string, any>();
}

function mockD1() {
  return {
    prepare: vi.fn(),
    exec: vi.fn(),
  } as any;
}

function extractErrorCode(str) {
  const match = str.match(/\(ErrorCode:\s*(\d+)\)/);
  return match ? Number(match[1]) : null;
}

beforeAll(() => {
  vi.mock("../orm.wasm", () => ({ default: new ArrayBuffer(0) }));
});

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
          .method("method", HttpVerb.DELETE, false, [], "Void")
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
          .method("method", HttpVerb.POST, true, [], "Void")
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
          .method("method", HttpVerb.POST, false, [], "Void")
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
          .method("method", HttpVerb.POST, false, [], "Void")
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
          .method("method", HttpVerb.POST, true, [], "Void")
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
          .method("method", HttpVerb.POST, false, [], "Void")
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
  test("Exits early on Model", async () => {
    // Arrange
    const env = mockWranglerEnv();
    const ast = createAst({
      models: [
        ModelBuilder.model("Foo")
          .idPk()
          .method("method", HttpVerb.POST, true, [], "Void")
          .build(),
      ],
    });
    const constructorRegistry = createCtorReg();
    const app = new CloesceApp(ast, constructorRegistry, env);
    const request = createRequest("http://foo.com/api/Foo/method", "POST");
    const di = createDi();
    const d1 = mockD1();

    class Foo {}

    app.onNamespace(Foo, async () => {
      return HttpResult.fail(500, "oogly boogly");
    });

    // Act
    const res = await (app as any).router(
      request,
      env,
      ast,
      constructorRegistry,
      di,
      d1,
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
          .method("method", HttpVerb.POST, true, [], "Void")
          .build(),
      ],
    });
    const constructorRegistry = createCtorReg();
    const app = new CloesceApp(ast, constructorRegistry, env);
    const request = createRequest("http://foo.com/api/Foo/method", "POST");
    const di = createDi();
    const d1 = mockD1();

    class Foo {}

    app.onNamespace(Foo, async () => {
      return HttpResult.fail(500, "oogly boogly");
    });

    // Act
    const res = await (app as any).router(
      request,
      env,
      ast,
      constructorRegistry,
      di,
      d1,
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
      .method("method", HttpVerb.POST, false, [], "Void")
      .build();
    const ast = createAst({
      models: [model],
    });

    class Foo {}
    const ctorReg = createCtorReg([Foo]);
    const route: MatchedRoute = {
      kind: "model",
      namespace: Foo.name,
      model,
      method: model.methods["method"],
      primaryKey: null,
      keyParams: {},
    };

    // Act
    const res = await _cloesceInternal.validateRequest(
      request,
      ast,
      ctorReg,
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
      .method("method", HttpVerb.POST, true, [], "Void")
      .build();
    const ast = createAst({
      models: [model],
    });

    class Foo {}
    const ctorReg = createCtorReg([Foo]);
    const route: MatchedRoute = {
      kind: "model",
      namespace: Foo.name,
      method: model.methods["method"],
      primaryKey: null,
      keyParams: {},
    };

    // Act
    const res = await _cloesceInternal.validateRequest(
      request,
      ast,
      ctorReg,
      route,
    );

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.RequestMissingBody,
    );
  });

  test("Request Body Missing Parameters => 400", async () => {
    // Arrange
    const request = createRequest("http://foo.com/api/Foo/method", "POST", {});
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method(
        "method",
        HttpVerb.POST,
        true,
        [
          {
            name: "missingParam",
            cidl_type: "Integer",
          },
        ],
        "Void",
      )
      .build();
    const ast = createAst({
      models: [model],
    });

    class Foo {}
    const ctorReg = createCtorReg([Foo]);
    const route: MatchedRoute = {
      kind: "model",
      namespace: Foo.name,
      method: model.methods["method"],
      primaryKey: null,
      keyParams: {},
    };

    // Act
    const res = await _cloesceInternal.validateRequest(
      request,
      ast,
      ctorReg,
      route,
    );

    // Assert
    expect(res.isLeft()).toBe(true);
    expect(extractErrorCode(res.unwrapLeft().message)).toEqual(
      RouterError.RequestBodyMissingParameters,
    );
  });

  class Scalar {
    id: number;
    manyScalarsId: number;
  }

  class ManyScalars {
    id: number;
    scalars: Scalar[];
  }

  const now = Date.now();
  const cases: {
    params: NamedTypedValue[];
    jsonValue: any;
    instanceValues: any;
    models?: Model[];
    noGetRequests?: boolean; // TODO: allow
    ctorReg?: Record<string, new () => any>;
  }[] = [
    // // Primitives
    {
      params: [
        {
          name: "int",
          cidl_type: "Integer",
        },
        {
          name: "string",
          cidl_type: "Text",
        },
        {
          name: "bool",
          cidl_type: "Boolean",
        },
        {
          name: "date",
          cidl_type: "DateIso",
        },
        {
          name: "float",
          cidl_type: "Real",
        },
      ],
      jsonValue: {
        int: "0",
        string: "hello",
        bool: "false",
        date: new Date(now).toISOString(),
        float: "0.99",
      },
      instanceValues: {
        int: 0,
        string: "hello",
        bool: false,
        date: new Date(now),
        float: 0.99,
      },
    },

    // // Data Sources
    {
      params: [
        {
          name: "ds",
          cidl_type: { DataSource: "TestCase" },
        },
      ],
      jsonValue: {
        ds: "none",
      },
      instanceValues: {
        ds: "none",
      },
    },

    // Models, Partials
    {
      params: [
        {
          name: "scalar",
          cidl_type: { Object: "Scalar" },
        },
        {
          name: "manyScalars",
          cidl_type: { Object: "ManyScalars" },
        },
        {
          name: "partialScalar",
          cidl_type: { Partial: "Scalar" },
        },
        {
          name: "partialManyScalars",
          cidl_type: { Partial: "ManyScalars" },
        },
      ],
      jsonValue: {
        scalar: {
          id: "0",
          manyScalarsId: "0",
        },
        manyScalars: {
          id: "0",
          scalars: [
            {
              id: "1",
              manyScalarsId: "0",
            },
            {
              id: "2",
              manyScalarsId: "0",
            },
          ],
        },
        partialScalar: {},
        partialManyScalars: {
          id: "1234",
        },
      },
      instanceValues: {
        scalar: Object.assign(new Scalar(), { id: 0, manyScalarsId: 0 }),
        manyScalars: Object.assign(new ManyScalars(), {
          id: 0,
          scalars: [
            Object.assign(new Scalar(), { id: 1, manyScalarsId: 0 }),
            Object.assign(new Scalar(), { id: 2, manyScalarsId: 0 }),
          ],
        }),
        partialScalar: {},
        partialManyScalars: {
          id: 1234,
          scalars: [],
        },
      },
      models: [
        ModelBuilder.model("Scalar")
          .idPk()
          .col("manyScalarsId", "Integer", "ManyScalars")
          .build(),
        ModelBuilder.model("ManyScalars")
          .idPk()
          .navP("scalars", "Scalar", {
            OneToMany: { column_reference: "manyScalarsId" },
          })
          .build(),
      ],
      ctorReg: {
        Scalar: Scalar,
        ManyScalars: ManyScalars,
      },
      noGetRequests: true,
    },
  ];

  const expandedCases = cases.flatMap((testCase) => {
    const canBeGetRequest = testCase.noGetRequests ? [false] : [true, false];
    return canBeGetRequest.flatMap((isGetRequest) =>
      [true, false].map((isSetToNull) => {
        let params = structuredClone(testCase.params);
        let jsonValue = structuredClone(testCase.jsonValue);
        let instanceValues = structuredClone(testCase.instanceValues);

        // Set everything to null and nullable
        if (isSetToNull) {
          for (const param of params) {
            param.cidl_type = { Nullable: param.cidl_type };
          }
          for (const value in jsonValue) {
            jsonValue[value] = null;
          }
          for (const value in instanceValues) {
            instanceValues[value] = null;
          }
        }

        return {
          params,
          jsonValue,
          instanceValues,
          isGetRequest,
          isSetToNull,
          models: testCase.models,
          ctorReg: testCase.ctorReg,
        };
      }),
    );
  });

  test.each(expandedCases)("validates parameters %#", async (testCase) => {
    // Arrange
    const model = ModelBuilder.model("TestCase")
      .idPk()
      .method(
        "testMethod",
        testCase.isGetRequest ? HttpVerb.GET : HttpVerb.POST,
        true,
        testCase.params,
        "Void",
      )
      .build();
    const ast = createAst({
      models: [model, ...(testCase.models ?? [])],
    });

    const url = new URL("https://foo.com/api");
    if (testCase.isGetRequest) {
      for (const key in testCase.jsonValue) {
        url.searchParams.set(key, testCase.jsonValue[key]);
      }
    }

    const request = createRequest(
      url.toString(),
      testCase.isGetRequest ? "GET" : "POST",
      testCase.isGetRequest ? undefined : testCase.jsonValue,
    );

    const route: MatchedRoute = {
      kind: "model",
      namespace: "TestCase",
      model,
      method: model.methods["testMethod"],
      primaryKey: null,
      keyParams: {},
    };

    // Act
    const res = await _cloesceInternal.validateRequest(
      request,
      ast,
      testCase.ctorReg ?? {},
      route,
    );

    // Assert
    expect(res.isRight()).toBe(true);
    expect(res.unwrap().params).toEqual(testCase.instanceValues);
  });
});

describe("Method Middleware", () => {
  test("Exits early", async () => {
    // Arrange
    const env = mockWranglerEnv();
    const ast = createAst({
      models: [
        ModelBuilder.model("Foo")
          .idPk()
          .method("method", HttpVerb.POST, true, [], "Void")
          .build(),
      ],
    });
    const constructorRegistry = createCtorReg();
    const app = new CloesceApp(ast, constructorRegistry, env);
    const request = createRequest(
      "http://foo.com/api/Foo/method",
      "POST",
      JSON.stringify({}),
    );

    const di = createDi();
    const d1 = mockD1();

    class Foo {
      method() {}
    }

    app.onMethod(Foo, "method", async () => {
      return HttpResult.fail(500, "oogly boogly");
    });

    // Act
    const res = await (app as any).router(
      request,
      env,
      ast,
      constructorRegistry,
      di,
      d1,
    );

    // Assert
    expect(res.status).toBe(500);
    expect(res.message).toBe("oogly boogly");
  });
});

describe("Method Dispatch", () => {
  test("Missing Dependency => 500", async () => {
    // Arrange
    const model = ModelBuilder.model("Foo")
      .idPk()
      .method(
        "method",
        HttpVerb.POST,
        true,
        [
          {
            name: "db",
            cidl_type: { Inject: "D1Database" },
          },
        ],
        "Void",
      )
      .build();

    const di = createDi();
    const route: MatchedRoute = {
      kind: "model",
      namespace: "Foo",
      method: model.methods["method"],
      primaryKey: null,
      keyParams: {},
    };

    // Act
    const res = await _cloesceInternal.methodDispatch({}, di, route, {});

    // Assert
    expect(res.ok).toBe(false);
    expect(extractErrorCode(res.message)).toBe(RouterError.MissingDependency);
  });

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
      .method("testMethod", HttpVerb.GET, true, [], "Void")
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
      .method("testMethod", HttpVerb.GET, true, [], { HttpResult: "Void" })
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
      .method("testMethod", HttpVerb.GET, true, [], "Text")
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
      .method("testMethod", HttpVerb.GET, true, [], "Text")
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
