import { describe, test, expect, vi, beforeEach } from "vitest";
import { _cloesceInternal } from "../src/router/router";
import {
  CloesceAst,
  HttpVerb,
  Model,
  NULL_DATA_SOURCE,
  NamedTypedValue,
} from "../src/common";
import { IncludeTree } from "../src/ui/backend";
import { CrudContext } from "../src/router/crud";
import { fromSql } from "../src/router/wasm";
import fs from "fs";
import path from "path";

const makeAst = (methods: Record<string, any>): CloesceAst => ({
  wrangler_env: { name: "", source_path: "./" },
  version: "",
  project_name: "",
  language: "TypeScript",
  models: {
    Horse: {
      name: "Horse",
      attributes: [],
      primary_key: { name: "_id", cidl_type: "Integer" },
      navigation_properties: [],
      data_sources: {},
      cruds: [],
      methods,
      source_path: "",
    },
  },
  poos: {},
});

const makeRequest = (url: string, method?: string, body?: any) =>
  new Request(url, {
    method,
    body: body && JSON.stringify(body),
  });

beforeEach(() => {
  vi.mock("../orm.wasm", () => ({ default: new ArrayBuffer(0) }));
});

describe("Router Error States", () => {
  const baseUrl = "http://foo.com/api";

  test("404 on route missing model/method", () => {
    const result = _cloesceInternal.matchRoute(
      makeRequest(baseUrl),
      makeAst({}),
      "/api",
    );
    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Expected /model/method or /model/:id/method /api`,
    });
  });

  test("404 on unknown model", () => {
    const url = `${baseUrl}/Dog/woof`;
    const result = _cloesceInternal.matchRoute(
      makeRequest(url),
      makeAst({}),
      "/api",
    );
    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Unknown model Dog /api/Dog/woof`,
    });
  });

  test("404 on unknown method", () => {
    const url = `${baseUrl}/Horse/neigh`;
    const result = _cloesceInternal.matchRoute(
      makeRequest(url),
      makeAst({}),
      "/api",
    );
    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Unknown method Horse.neigh /api/Horse/neigh`,
    });
  });

  test("404 on mismatched HTTP verb", () => {
    const url = `${baseUrl}/Horse/0/neigh`;
    const ast = makeAst({
      neigh: {
        name: "",
        is_static: false,
        http_verb: HttpVerb.PUT,
        return_type: null,
        parameters: [],
      },
    });
    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");
    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Unmatched HTTP method /api/Horse/0/neigh`,
    });
  });
});

describe("Router Success States", () => {
  const methods = {
    neigh: {
      name: "neigh",
      is_static: false,
      http_verb: HttpVerb.GET,
      return_type: null,
      parameters: [],
    },
  };
  const ast = makeAst(methods);

  test("returns model on static route", () => {
    const url = "http://foo.com/api/Horse/neigh";
    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");
    expect(result.value).toStrictEqual({
      model: ast.models.Horse,
      method: ast.models.Horse.methods.neigh,
      id: null,
    });
  });

  test("returns model on instantiated route", () => {
    const url = "http://foo.com/api/Horse/0/neigh";
    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");
    expect(result.value).toStrictEqual({
      model: ast.models.Horse,
      method: ast.models.Horse.methods.neigh,
      id: "0",
    });
  });
});

describe("Validate Request Error States", () => {
  const emptyAst: CloesceAst = {
    models: {},
    wrangler_env: { name: "", source_path: "./" },
    version: "",
    project_name: "",
    language: "TypeScript",
    poos: {},
  };
  const emptyModel: Model = {
    name: "",
    primary_key: { name: "", cidl_type: "Void" },
    attributes: [],
    navigation_properties: [],
    methods: {},
    cruds: [],
    data_sources: {},
    source_path: "",
  };

  test("instantiated methods require id", async () => {
    const result = await _cloesceInternal.validateRequest(
      makeRequest("http://foo.com/api/Horse/0/neigh"),
      emptyAst,
      emptyModel,
      {
        name: "",
        is_static: false,
        http_verb: HttpVerb.GET,
        return_type: null,
        parameters: [],
      },
      null,
    );

    expect(result.value).toStrictEqual({
      ok: false,
      status: 400,
      message:
        "Invalid Request Body: Id's are required for instantiated methods.",
    });
  });

  test("non-GET requests require JSON body", async () => {
    const result = await _cloesceInternal.validateRequest(
      makeRequest("http://foo.com/api/Horse/0/neigh"),
      emptyAst,
      emptyModel,
      {
        name: "",
        is_static: true,
        http_verb: HttpVerb.PATCH,
        return_type: null,
        parameters: [],
      },
      null,
    );

    expect(result.value).toStrictEqual({
      ok: false,
      status: 400,
      message: "Invalid Request Body: Could not retrieve JSON body.",
    });
  });

  test.each([
    { method: HttpVerb.POST, body: {}, message: "Missing parameters" },
    {
      method: HttpVerb.POST,
      body: { id: "notNumber" },
      message: "Invalid parameters",
    },
    {
      method: HttpVerb.GET,
      query: "id=notNumber",
      message: "Invalid parameters",
    },
  ])("invalid params: $message", async ({ method, body, query, message }) => {
    const url = query
      ? `http://foo.com/api/Horse/neigh?${query}`
      : "http://foo.com/api/Horse/neigh";
    const request = makeRequest(url, method, body);
    const ast = makeAst({
      neigh: {
        name: "neigh",
        is_static: true,
        http_verb: method,
        return_type: null,
        parameters: [{ name: "id", cidl_type: "Integer", nullable: false }],
      },
    });

    const result = await _cloesceInternal.validateRequest(
      request,
      ast,
      ast.models.Horse,
      ast.models.Horse.methods.neigh,
      "0",
    );

    expect(result.value).toStrictEqual({
      ok: false,
      status: 400,
      message: `Invalid Request Body: ${message}.`,
    });
  });
});

describe("Validate Request Success States", () => {
  const inputs: { typed_value: NamedTypedValue; value: any }[] = [
    { typed_value: { name: "id", cidl_type: "Integer" }, value: "1" },
    { typed_value: { name: "lastName", cidl_type: "Text" }, value: "pumpkin" },
    { typed_value: { name: "gpa", cidl_type: "Real" }, value: "4.0" },
    {
      typed_value: { name: "horse", cidl_type: { Object: "Horse" } },
      value: { _id: 1 },
    },
    {
      typed_value: { name: "horse", cidl_type: { Partial: "Horse" } },
      value: {},
    },
  ];

  const expanded = inputs.flatMap((i) =>
    [true, false].flatMap((isGet) =>
      [true, false].map((nullable) => ({
        ...i,
        isGet: isGet && typeof i.typed_value.cidl_type === "string",
        value: nullable ? null : i.value,
        typed_value: {
          ...i.typed_value,
          cidl_type: nullable
            ? { Nullable: i.typed_value.cidl_type }
            : i.typed_value.cidl_type,
        },
      })),
    ),
  );

  test.each(expanded)("accepts valid input %#", async (arg) => {
    const url = arg.isGet
      ? `http://foo.com/api/Horse/neigh?${arg.typed_value.name}=${arg.value}`
      : "http://foo.com/api/Horse/neigh";
    const request = makeRequest(
      url,
      arg.isGet ? undefined : "POST",
      arg.isGet ? undefined : { [arg.typed_value.name]: arg.value },
    );
    const ast = makeAst({
      neigh: {
        name: "neigh",
        is_static: true,
        http_verb: arg.isGet ? HttpVerb.GET : HttpVerb.POST,
        return_type: null,
        parameters: [
          arg.typed_value,
          { name: "db", cidl_type: { Inject: "Env" }, nullable: false },
        ],
      },
    });

    const result = await _cloesceInternal.validateRequest(
      request,
      ast,
      ast.models.Horse,
      ast.models.Horse.methods.neigh,
      null,
    );

    expect(result.value).toEqual({
      params: {
        [arg.typed_value.name]: arg.isGet ? String(arg.value) : arg.value,
        dataSource: arg.isGet ? undefined : NULL_DATA_SOURCE,
      },
      dataSource: NULL_DATA_SOURCE,
    });
  });
});

describe("methodDispatch", () => {
  const makeMethod = (overrides: Partial<any> = {}) => ({
    name: "testMethod",
    is_static: true,
    http_verb: HttpVerb.GET,
    return_type: null,
    parameters: [],
    ...overrides,
  });

  const makeMockD1 = () => ({
    prepare: vi.fn(),
    batch: vi.fn(),
    exec: vi.fn(),
    withSession: vi.fn(),
    dump: vi.fn(),
  });
  const envMeta = { envName: "Env", dbName: "db" };
  const makeRegistry = () => new Map([["Env", { db: makeMockD1() }]]);

  test("returns 200 with no data when return_type null", async () => {
    const instance = { testMethod: vi.fn().mockResolvedValue("ignored") };
    const result = await _cloesceInternal.methodDispatch(
      CrudContext.fromInstance(makeMockD1(), instance, vi.fn()),
      makeRegistry(),
      envMeta,
      makeMethod(),
      {},
    );
    expect(instance.testMethod).toHaveBeenCalled();
    expect(result).toStrictEqual({ ok: true, status: 200 });
  });

  test("wraps result when return_type { HttpResult }", async () => {
    const instance = {
      testMethod: vi
        .fn()
        .mockResolvedValue({ ok: true, status: 200, data: "wrapped" }),
    };
    const result = await _cloesceInternal.methodDispatch(
      CrudContext.fromInstance(makeMockD1(), instance, vi.fn()),
      makeRegistry(),
      envMeta,
      makeMethod({ return_type: { HttpResult: null } }),
      {},
    );
    expect(result).toStrictEqual({ ok: true, status: 200, data: "wrapped" });
  });

  test("wraps raw value when return_type is value type", async () => {
    const instance = { testMethod: vi.fn().mockResolvedValue("neigh") };
    const result = await _cloesceInternal.methodDispatch(
      CrudContext.fromInstance(makeMockD1(), instance, vi.fn()),
      makeRegistry(),
      envMeta,
      makeMethod({ return_type: "Text" }),
      {},
    );
    expect(result).toStrictEqual({ ok: true, status: 200, data: "neigh" });
  });

  test("injects default d1 param", async () => {
    const instance = { testMethod: vi.fn().mockResolvedValue("used d1") };
    const ireg = makeRegistry();
    const result = await _cloesceInternal.methodDispatch(
      CrudContext.fromInstance(makeMockD1(), instance, vi.fn()),
      ireg,
      envMeta,
      makeMethod({ return_type: "Text", parameters: [{ name: "database" }] }),
      {},
    );
    expect(instance.testMethod).toHaveBeenCalledWith(ireg.get("Env"));
    expect(result).toStrictEqual({ ok: true, status: 200, data: "used d1" });
  });

  test("handles thrown errors", async () => {
    const errInstance = {
      testMethod: vi.fn().mockImplementation(() => {
        throw new Error("boom");
      }),
    };
    const strInstance = {
      testMethod: vi.fn().mockImplementation(() => {
        throw "stringError";
      }),
    };
    const ctx = CrudContext.fromInstance(makeMockD1(), errInstance, vi.fn());
    const method = makeMethod({ return_type: "Text" });
    const result1 = await _cloesceInternal.methodDispatch(
      ctx,
      makeRegistry(),
      envMeta,
      method,
      {},
    );
    const result2 = await _cloesceInternal.methodDispatch(
      CrudContext.fromInstance(makeMockD1(), strInstance, vi.fn()),
      makeRegistry(),
      envMeta,
      method,
      {},
    );

    expect(result1).toStrictEqual({
      ok: false,
      status: 500,
      message: "Uncaught exception in method dispatch: boom",
    });
    expect(result2).toStrictEqual({
      ok: false,
      status: 500,
      message: "Uncaught exception in method dispatch: stringError",
    });
  });
});

describe("modelsFromSql", () => {
  test("handles recursive navigation properties", async () => {
    const wasm = await WebAssembly.instantiate(
      fs.readFileSync(path.resolve("./dist/orm.wasm")),
      {},
    );

    const modelName = "Horse";
    const likeModelName = "Like";
    const ctor = {
      [modelName]: class {
        id?: string;
        name?: string;
        bio?: string | null;
        likes?: any[];
      },
      [likeModelName]: class {
        id?: string;
        horseId1?: string;
        horseId2?: string;
        horse2?: any;
      },
    };

    const ast: CloesceAst = {
      wrangler_env: { name: "Env", source_path: "./" },
      models: {
        [modelName]: {
          name: modelName,
          attributes: [
            {
              value: { name: "name", cidl_type: "Text" },
              foreign_key_reference: null,
            },
            {
              value: { name: "bio", cidl_type: { Nullable: "Text" } },
              foreign_key_reference: null,
            },
          ],
          navigation_properties: [
            {
              var_name: "likes",
              model_name: likeModelName,
              kind: { OneToMany: { reference: "horseId1" } },
            },
          ],
          primary_key: { name: "id", cidl_type: "Integer" },
          data_sources: {},
          methods: {},
          cruds: [],
          source_path: "",
        },
        [likeModelName]: {
          name: likeModelName,
          attributes: [
            {
              value: { name: "horseId1", cidl_type: "Integer" },
              foreign_key_reference: modelName,
            },
            {
              value: { name: "horseId2", cidl_type: "Integer" },
              foreign_key_reference: modelName,
            },
          ],
          navigation_properties: [
            {
              var_name: "horse2",
              model_name: modelName,
              kind: { OneToOne: { reference: "horseId2" } },
            },
          ],
          primary_key: { name: "id", cidl_type: "Integer" },
          data_sources: {},
          methods: {},
          cruds: [],
          source_path: "",
        },
      },
      version: "",
      project_name: "",
      language: "TypeScript",
      poos: {},
    };

    await _cloesceInternal.RuntimeContainer.init(ast, ctor, wasm.instance);

    const records = [
      {
        "Horse.id": "1",
        "Horse.name": "Lightning",
        "Horse.bio": "Fast horse",
        "Horse.likes.id": "10",
        "Horse.likes.horseId1": "1",
        "Horse.likes.horseId2": "2",
        "Horse.likes.horse2.id": "2",
        "Horse.likes.horse2.name": "Thunder",
        "Horse.likes.horse2.bio": "Strong horse",
      },
      {
        "Horse.id": "1",
        "Horse.name": "Lightning",
        "Horse.bio": "Fast horse",
        "Horse.likes.id": "11",
        "Horse.likes.horseId1": "1",
        "Horse.likes.horseId2": "3",
        "Horse.likes.horse2.id": "3",
        "Horse.likes.horse2.name": "Storm",
        "Horse.likes.horse2.bio": null,
      },
    ];

    const includeTree: IncludeTree<any> = { likes: { horse2: {} } };
    const result = fromSql(ctor[modelName], records, includeTree);

    expect(result.value.length).toBe(1);
    const horse: any = result.value[0];
    expect(horse).toMatchObject({
      id: "1",
      name: "Lightning",
      bio: "Fast horse",
      likes: [
        {
          id: "10",
          horseId1: "1",
          horseId2: "2",
          horse2: { id: "2", name: "Thunder", bio: "Strong horse" },
        },
        {
          id: "11",
          horseId1: "1",
          horseId2: "3",
          horse2: { id: "3", name: "Storm", bio: null },
        },
      ],
    });
  });
});
