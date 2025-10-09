import { describe, test, expect, vi } from "vitest";
import { _cloesceInternal } from "../src/runtime/runtime";
import { CloesceAst, HttpVerb, NamedTypedValue } from "../src/common";
import { modelsFromSql } from "../src/runtime/runtime";
import { IncludeTree } from "../src";
import fs from "fs";
import path from "path";

const makeAst = (methods: Record<string, any>): CloesceAst => ({
  wrangler_env: {
    name: "",
    source_path: "./",
  },
  version: "",
  project_name: "",
  language: "TypeScript",
  models: {
    Horse: {
      name: "",
      attributes: [],
      primary_key: {
        name: "void",
        cidl_type: "Integer",
      },
      navigation_properties: [],
      data_sources: {},
      methods,
      source_path: "",
    },
  },
  poos: {},
});

const makeRequest = (url: string, method?: string, body?: any) =>
  new Request(
    url,
    method ? { method, body: body && JSON.stringify(body) } : undefined,
  );

//
// Router Tests
//

describe("Router Error States", () => {
  test("Router returns 404 on route missing model, method", () => {
    const url = "http://foo.com/api";
    const result = _cloesceInternal.matchRoute(
      makeRequest(url),
      makeAst({}),
      "/api",
    );

    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Expected /model/method or /model/:id/method /api`,
    });
  });

  test("Router returns 404 on unknown model", () => {
    const url = "http://foo.com/api/Dog/woof";
    const ast = makeAst({});

    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");

    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Unknown model Dog /api/Dog/woof`,
    });
  });

  test("Router returns 404 on unknown method", () => {
    const url = "http://foo.com/api/Horse/neigh";
    const ast = makeAst({});

    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");

    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Unknown method Horse.neigh /api/Horse/neigh`,
    });
  });

  test("Router returns 404 on mismatched HTTP verb", () => {
    const url = "http://foo.com/api/Horse/0/neigh";
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

  test("Router returns model on static route", () => {
    const url = "http://foo.com/api/Horse/neigh";
    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");

    expect(result.value).toStrictEqual({
      model: ast.models.Horse,
      method: ast.models.Horse.methods.neigh,
      id: null,
    });
  });

  test("Router returns model on instantiated route", () => {
    const url = "http://foo.com/api/Horse/0/neigh";
    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");

    expect(result.value).toStrictEqual({
      model: ast.models.Horse,
      method: ast.models.Horse.methods.neigh,
      id: "0",
    });
  });
});

//
// Validate Request Tests
//

describe("Validate Request Error States", () => {
  test("Instantiated methods require id", async () => {
    const request = makeRequest("http://foo.com/api/Horse/0/neigh");

    const result = await _cloesceInternal.validateRequest(
      request,
      {
        models: {},
        wrangler_env: { name: "", source_path: "./" },
        version: "",
        project_name: "",
        language: "TypeScript",
        poos: {},
      },
      {
        name: "",
        primary_key: {
          name: "",
          cidl_type: "Void",
        },
        attributes: [],
        navigation_properties: [],
        methods: {},
        data_sources: {},
        source_path: "",
      },
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

  test("Non-GET requests require JSON body", async () => {
    const request = makeRequest("http://foo.com/api/Horse/0/neigh");

    const result = await _cloesceInternal.validateRequest(
      request,
      {
        models: {},
        wrangler_env: { name: "", source_path: "./" },
        version: "",
        project_name: "",
        language: "TypeScript",
        poos: {},
      },
      {
        name: "",
        primary_key: {
          name: "",
          cidl_type: "Void",
        },
        attributes: [],
        navigation_properties: [],
        methods: {},
        data_sources: {},
        source_path: "",
      },
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

  const paramTests = [
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
  ];

  test.each(paramTests)(
    "Request validation: $message",
    async ({ method, body, query, message }) => {
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
    },
  );
});

describe("Validate Request Success States", () => {
  const input: { typed_value: NamedTypedValue; value: string }[] = [
    { typed_value: { name: "id", cidl_type: "Integer" }, value: "1" },
    { typed_value: { name: "lastName", cidl_type: "Text" }, value: "pumpkin" },
    { typed_value: { name: "gpa", cidl_type: "Real" }, value: "4.0" },
  ];

  const expanded = input.flatMap((i) =>
    [true, false].flatMap((is_get) =>
      [true, false].map((nullable) => ({
        ...i,
        is_get,
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

  test.each(expanded)("input is accepted %#", async (arg) => {
    const url = arg.is_get
      ? `http://foo.com/api/Horse/neigh?${arg.typed_value.name}=${arg.value}`
      : "http://foo.com/api/Horse/neigh";
    const request = makeRequest(
      url,
      arg.is_get ? undefined : "POST",
      arg.is_get ? undefined : { [arg.typed_value.name]: arg.value },
    );
    const ast = makeAst({
      neigh: {
        name: "neigh",
        is_static: true,
        http_verb: arg.is_get ? HttpVerb.GET : HttpVerb.POST,
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
      "0",
    );

    expect(result.value).toEqual([
      { [arg.typed_value.name]: arg.is_get ? String(arg.value) : arg.value },
      null,
    ]);
  });
});

//
// methodDispatch Tests
//

describe("methodDispatch", () => {
  const makeMethod = (overrides: Partial<any> = {}) => ({
    name: "testMethod",
    is_static: true,
    http_verb: HttpVerb.GET,
    return_type: null,
    parameters: [],
    ...overrides,
  });

  const makeMockD1 = (): any => ({
    prepare: vi.fn(),
    batch: vi.fn(),
    exec: vi.fn(),
    withSession: vi.fn(),
    dump: vi.fn(),
  });

  const envMeta = { envName: "Env", dbName: "db" };

  const makeInstanceRegistry = () => new Map([["Env", { db: makeMockD1() }]]);

  test("returns 200 with no data when return_type is null", async () => {
    const instance = { testMethod: vi.fn().mockResolvedValue("ignored") };
    const method = makeMethod({ return_type: null });
    const params = {};

    const result = await _cloesceInternal.methodDispatch(
      instance,
      makeInstanceRegistry(),
      envMeta,
      method,
      params,
    );

    expect(instance.testMethod).toHaveBeenCalledWith();
    expect(result).toStrictEqual({ ok: true, status: 200 });
  });

  test("wraps result in HttpResult when return_type is { HttpResult }", async () => {
    const instance = {
      testMethod: vi
        .fn()
        .mockResolvedValue({ ok: true, status: 200, data: "already wrapped" }),
    };
    const method = makeMethod({ return_type: { HttpResult: null } });
    const params = {};

    const result = await _cloesceInternal.methodDispatch(
      instance,
      makeInstanceRegistry(),
      envMeta,
      method,
      params,
    );

    expect(result).toStrictEqual({
      ok: true,
      status: 200,
      data: "already wrapped",
    });
  });

  test("wraps raw value when return_type is a value type", async () => {
    const instance = { testMethod: vi.fn().mockResolvedValue("neigh") };
    const method = makeMethod({ return_type: "Text" });
    const params = {};

    const result = await _cloesceInternal.methodDispatch(
      instance,
      makeInstanceRegistry(),
      envMeta,
      method,
      params,
    );

    expect(result).toStrictEqual({ ok: true, status: 200, data: "neigh" });
  });

  test("supplies d1 as default param when missing in params", async () => {
    const instance = { testMethod: vi.fn().mockResolvedValue("used d1") };
    const method = makeMethod({
      return_type: "Text",
      parameters: [{ name: "database" }],
    });
    const params = {};
    let ireg = makeInstanceRegistry();

    const result = await _cloesceInternal.methodDispatch(
      instance,
      ireg,
      envMeta,
      method,
      params,
    );

    expect(instance.testMethod).toHaveBeenCalledWith(ireg.get("Env"));
    expect(result).toStrictEqual({ ok: true, status: 200, data: "used d1" });
  });

  test("returns 500 on thrown Error", async () => {
    const instance = {
      testMethod: vi.fn().mockImplementation(() => {
        throw new Error("boom");
      }),
    };
    const method = makeMethod({ return_type: "Text" });
    const params = {};

    const result = await _cloesceInternal.methodDispatch(
      instance,
      makeInstanceRegistry(),
      envMeta,
      method,
      params,
    );

    expect(result).toStrictEqual({
      ok: false,
      status: 500,
      message: "Uncaught exception in method dispatch: boom",
    });
  });

  test("returns 500 on thrown non-Error value", async () => {
    const instance = {
      testMethod: vi.fn().mockImplementation(() => {
        throw "stringError";
      }),
    };
    const method = makeMethod({ return_type: "Text" });
    const params = {};

    const result = await _cloesceInternal.methodDispatch(
      instance,
      makeInstanceRegistry(),
      envMeta,
      method,
      params,
    );

    expect(result).toStrictEqual({
      ok: false,
      status: 500,
      message: "Uncaught exception in method dispatch: stringError",
    });
  });
});

//
// modelsFromSql Tests
//
describe("modelsFromSql", () => {
  // We are really just testing the recursive instantiation here, modelsFromSql
  // does plentiful unit tests inside of the Rust project
  test("handles recursive navigation properties", async () => {
    // Arrange
    const wasmPath = path.resolve(
      __dirname,
      "../../../runtime/target/wasm32-unknown-unknown/release/runtime.wasm",
    );
    const wasm = await WebAssembly.instantiate(fs.readFileSync(wasmPath), {});

    const modelName = "Horse";
    const likeModelName = "Like";

    const constructorRegistry = {
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
          source_path: "",
        },
      },
      version: "",
      project_name: "",
      language: "TypeScript",
      poos: {},
    };

    await _cloesceInternal.RuntimeContainer.init(
      ast,
      constructorRegistry,
      wasm.instance,
    );

    // Simulate a view result with nested data: Horse -> likes (Like[]) -> horse2 (Horse)
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

    const includeTree: IncludeTree<any> = {
      likes: {
        horse2: {},
      },
    };

    // Act
    const result = modelsFromSql(
      constructorRegistry[modelName],
      records,
      includeTree,
    );

    // Assert
    expect(result.length).toBe(1);

    const horse: any = result[0];
    expect(horse.id).toBe("1");
    expect(horse.name).toBe("Lightning");
    expect(horse.bio).toBe("Fast horse");

    expect(Array.isArray(horse.likes)).toBe(true);
    expect(horse.likes.length).toBe(2);

    const like1 = horse.likes[0];
    expect(like1.id).toBe("10");
    expect(like1.horseId1).toBe("1");
    expect(like1.horseId2).toBe("2");

    expect(like1.horse2).toBeDefined();
    expect(like1.horse2.id).toBe("2");
    expect(like1.horse2.name).toBe("Thunder");
    expect(like1.horse2.bio).toBe("Strong horse");

    const like2 = horse.likes[1];
    expect(like2.id).toBe("11");
    expect(like2.horseId1).toBe("1");
    expect(like2.horseId2).toBe("3");

    expect(like2.horse2).toBeDefined();
    expect(like2.horse2.id).toBe("3");
    expect(like2.horse2.name).toBe("Storm");
    expect(like2.horse2.bio).toBeNull();
  });
});
