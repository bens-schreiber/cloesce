import { _cloesceInternal } from "../src/cloesce";
import {
  CloesceAst,
  DataSource,
  HttpVerb,
  ModelAttribute,
  NamedTypedValue,
  NavigationProperty,
} from "../src/common";

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
});

const makeRequest = (url: string, method?: string, body?: any) =>
  new Request(
    url,
    method ? { method, body: body && JSON.stringify(body) } : undefined,
  );

describe("Router Error States", () => {
  test("Router returns 404 on route missing model, method", () => {
    // Arrange
    const url = "http://foo.com/api";

    // Act
    const result = _cloesceInternal.matchRoute(
      makeRequest(url),
      makeAst({}),
      "/api",
    );

    // Assert
    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Expected /model/method or /model/:id/method /api`,
    });
  });

  test("Router returns 404 on unknown model", () => {
    // Arrange
    const url = "http://foo.com/api/Dog/woof";
    const ast = makeAst({});

    // Act
    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");

    // Assert
    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Unknown model Dog /api/Dog/woof`,
    });
  });

  test("Router returns 404 on unknown method", () => {
    // Arrange
    const url = "http://foo.com/api/Horse/neigh";
    const ast = makeAst({});

    // Act
    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");

    // Assert
    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Unknown method Horse.neigh /api/Horse/neigh`,
    });
  });

  test("Router returns 404 on mismatched HTTP verb", () => {
    // Arrange
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

    // Act
    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");

    // Assert
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
    // Arrange
    const url = "http://foo.com/api/Horse/neigh";

    // Act
    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");

    // Assert
    expect(result.value).toStrictEqual({
      model: ast.models.Horse,
      method: ast.models.Horse.methods.neigh,
      id: null,
    });
  });

  test("Router returns model on instantiated route", () => {
    // Arrange
    const url = "http://foo.com/api/Horse/0/neigh";

    // Act
    const result = _cloesceInternal.matchRoute(makeRequest(url), ast, "/api");

    // Assert
    expect(result.value).toStrictEqual({
      model: ast.models.Horse,
      method: ast.models.Horse.methods.neigh,
      id: "0",
    });
  });
});

describe("Validate Request Error States", () => {
  test("Instantiated methods require id", async () => {
    // Arrange
    const request = makeRequest("http://foo.com/api/Horse/0/neigh");

    // Act
    const result = await _cloesceInternal.validateRequest(
      request,
      {
        models: {},
        wrangler_env: { name: "", source_path: "./" },
        version: "",
        project_name: "",
        language: "TypeScript",
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

    // Assert
    expect(result.value).toStrictEqual({
      ok: false,
      status: 400,
      message:
        "Invalid Request Body: Id's are required for instantiated methods.",
    });
  });

  test("Non-GET requests require JSON body", async () => {
    // Arrange
    const request = makeRequest("http://foo.com/api/Horse/0/neigh");

    // Act
    const result = await _cloesceInternal.validateRequest(
      request,
      {
        models: {},
        wrangler_env: { name: "", source_path: "./" },
        version: "",
        project_name: "",
        language: "TypeScript",
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

    // Assert
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
      // Arrange
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

      // Act
      const result = await _cloesceInternal.validateRequest(
        request,
        ast,
        ast.models.Horse,
        ast.models.Horse.methods.neigh,
        "0",
      );

      // Assert
      expect(result.value).toStrictEqual({
        ok: false,
        status: 400,
        message: `Invalid Request Body: ${message}.`,
      });
    },
  );
});

describe("Validate Request Success States", () => {
  const input: {
    typed_value: NamedTypedValue;
    value: string;
  }[] = [
    {
      typed_value: { name: "id", cidl_type: "Integer" },
      value: "1",
    },
    {
      typed_value: { name: "lastName", cidl_type: "Text" },
      value: "pumpkin",
    },
    {
      typed_value: { name: "gpa", cidl_type: "Real" },
      value: "4.0",
    },
  ];

  // cartesian expansion for is_get and nullable
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
    // Arrange
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
          {
            name: "db",
            cidl_type: { Inject: "Env" },
            nullable: false,
          },
        ],
      },
    });

    // Act
    const result = await _cloesceInternal.validateRequest(
      request,
      ast,
      ast.models.Horse,
      ast.models.Horse.methods.neigh,
      "0",
    );

    // Assert
    expect(result.value).toEqual([
      {
        [arg.typed_value.name]: arg.is_get ? String(arg.value) : arg.value,
      },
      null,
    ]);
  });
});

describe("modelsFromSql", () => {
  const modelName = "Horse";
  const nestedModelName = "Rider";

  const constructorRegistry = {
    [modelName]: class {
      id?: string;
      name?: string;
      riders?: any[];
    },
    [nestedModelName]: class {
      id?: string;
      nickname?: string;
    },
  };

  const baseCidl: CloesceAst = {
    wrangler_env: {
      name: "Env",
      source_path: "./",
    },
    models: {
      [modelName]: {
        name: modelName,
        attributes: [
          {
            value: {
              name: "name",
              cidl_type: { Nullable: "Text" },
            },
            foreign_key_reference: null,
          },
        ],
        navigation_properties: [
          {
            var_name: "riders",
            model_name: nestedModelName,
            kind: { OneToMany: { reference: "id" } },
          },
        ],
        primary_key: {
          name: "id",
          cidl_type: "Integer",
        },
        data_sources: {},
        methods: {},
        source_path: "",
      },
      [nestedModelName]: {
        name: nestedModelName,
        attributes: [
          {
            value: {
              name: "nickname",
              cidl_type: { Nullable: "Text" },
            },
            foreign_key_reference: null,
          },
        ],
        primary_key: {
          name: "id",
          cidl_type: "Integer",
        },
        navigation_properties: [],
        data_sources: {},
        methods: {},
        source_path: "",
      },
    },
    version: "",
    project_name: "",
    language: "TypeScript",
  };

  test("returns empty array if no records", () => {
    // Arrange
    const records: any[] = [];

    // Act
    const result = _cloesceInternal._modelsFromSql(
      modelName,
      baseCidl,
      constructorRegistry,
      records,
      {},
    );

    // Assert
    expect(result).toEqual([]);
  });

  test("assigns scalar attributes and navigation arrays correctly", () => {
    // Arrange
    const records = [
      {
        Horse_id: "1",
        Horse_name: "Thunder",
        Rider_id: "r1",
        Rider_nickname: "Speedy",
      },
      {
        Horse_id: "1",
        Horse_name: "Thunder",
        Rider_id: "r2",
        Rider_nickname: "Flash",
      },
    ];
    const tree = { riders: {} };

    // Act
    const result = _cloesceInternal._modelsFromSql(
      modelName,
      baseCidl,
      constructorRegistry,
      records,
      tree,
    );
    const horse: any = result[0];

    // Assert
    expect(horse.id).toBe("1");
    expect(horse.name).toBe("Thunder");
    expect(Array.isArray(horse.riders)).toBe(true);
    expect(horse.riders.map((r: any) => r.id)).toEqual(
      expect.arrayContaining(["r1", "r2"]),
    );
  });

  test("handles prefixed columns correctly", () => {
    // Arrange
    const records = [{ Horse_id: "1", Horse_name: "Lightning" }];

    // Act
    const result = _cloesceInternal._modelsFromSql(
      modelName,
      baseCidl,
      constructorRegistry,
      records,
      {},
    );
    const horse: any = result[0];

    // Assert
    expect(horse.id).toBe("1");
    expect(horse.name).toBe("Lightning");
  });

  test("merges duplicate rows with arrays", () => {
    // Arrange
    const records = [
      {
        Horse_id: "1",
        Horse_name: "hoarse",
        Rider_id: "r1",
        Rider_nickname: "Speedy",
      },
      {
        Horse_id: "1",
        Horse_name: "hoarse",
        Rider_id: "r1",
        Rider_nickname: "Speedy",
      },
      {
        Horse_id: "1",
        Horse_name: "hoarse",
        Rider_id: "r2",
        Rider_nickname: "Flash",
      },
    ];
    const tree = { riders: {} };

    // Act
    const result = _cloesceInternal._modelsFromSql(
      modelName,
      baseCidl,
      constructorRegistry,
      records,
      tree,
    );
    const horse: any = result[0];

    // Assert
    expect(horse.riders.length).toBe(2);
    expect(horse.riders.map((r: any) => r.id)).toEqual(
      expect.arrayContaining(["r1", "r2"]),
    );
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

  const makeMockD1 = (): any => ({
    prepare: jest.fn(),
    batch: jest.fn(),
    exec: jest.fn(),
    withSession: jest.fn(),
    dump: jest.fn(),
  });

  const envMeta = {
    envName: "Env",
    dbName: "db",
  };

  const makeInstanceRegistry = () =>
    new Map([
      [
        "Env",
        {
          db: makeMockD1(),
        },
      ],
    ]);

  test("returns 200 with no data when return_type is null", async () => {
    // Arrange
    const instance = {
      testMethod: jest.fn().mockResolvedValue("ignored"),
    };
    const method = makeMethod({ return_type: null });
    const params = {};

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      makeInstanceRegistry(),
      envMeta,
      method,
      params,
    );

    // Assert
    expect(instance.testMethod).toHaveBeenCalledWith();
    expect(result).toStrictEqual({ ok: true, status: 200 });
  });

  test("wraps result in HttpResult when return_type is { HttpResult }", async () => {
    // Arrange
    const instance = {
      testMethod: jest.fn().mockResolvedValue({
        ok: true,
        status: 200,
        data: "already wrapped",
      }),
    };
    const method = makeMethod({ return_type: { HttpResult: null } });
    const params = {};

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      makeInstanceRegistry(),
      envMeta,
      method,
      params,
    );

    // Assert
    expect(result).toStrictEqual({
      ok: true,
      status: 200,
      data: "already wrapped",
    });
  });

  test("wraps raw value when return_type is a value type", async () => {
    // Arrange
    const instance = {
      testMethod: jest.fn().mockResolvedValue("neigh"),
    };
    const method = makeMethod({ return_type: "Text" });
    const params = {};

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      makeInstanceRegistry(),
      envMeta,
      method,
      params,
    );

    // Assert
    expect(result).toStrictEqual({ ok: true, status: 200, data: "neigh" });
  });

  test("supplies d1 as default param when missing in params", async () => {
    // Arrange
    const instance = {
      testMethod: jest.fn().mockResolvedValue("used d1"),
    };
    const method = makeMethod({
      return_type: "Text",
      parameters: [{ name: "database" }],
    });
    const params = {};
    let ireg = makeInstanceRegistry();

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      ireg,
      envMeta,
      method,
      params,
    );

    // Assert
    expect(instance.testMethod).toHaveBeenCalledWith(ireg.get("Env"));
    expect(result).toStrictEqual({ ok: true, status: 200, data: "used d1" });
  });

  test("returns 500 on thrown Error", async () => {
    // Arrange
    const instance = {
      testMethod: jest.fn().mockImplementation(() => {
        throw new Error("boom");
      }),
    };
    const method = makeMethod({ return_type: "Text" });
    const params = {};

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      makeInstanceRegistry(),
      envMeta,
      method,
      params,
    );

    // Assert
    expect(result).toStrictEqual({
      ok: false,
      status: 500,
      message: "Uncaught exception in method dispatch: boom",
    });
  });

  test("returns 500 on thrown non-Error value", async () => {
    // Arrange
    const instance = {
      testMethod: jest.fn().mockImplementation(() => {
        throw "stringError";
      }),
    };
    const method = makeMethod({ return_type: "Text" });
    const params = {};

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      makeInstanceRegistry(),
      envMeta,
      method,
      params,
    );

    // Assert
    expect(result).toStrictEqual({
      ok: false,
      status: 500,
      message: "Uncaught exception in method dispatch: stringError",
    });
  });
});
