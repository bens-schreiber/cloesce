import { _cloesceInternal } from "../src/cloesce";
import { HttpVerb, MetaCidl, NamedTypedValue } from "../src/common";

const makeCidl = (methods: Record<string, any>) => ({
  models: {
    Horse: {
      name: "",
      attributes: [],
      navigation_properties: [],
      data_sources: [],
      methods,
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
    const result = _cloesceInternal.matchRoute(makeRequest(url), "/api", {
      models: {},
    });

    // Assert
    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Expected /model/method or /model/:id/method /api`,
    });
  });

  test("Router returns 404 on unknown model", () => {
    // Arrange
    const url = "http://foo.com/api/Horse/neigh";
    const cidl = {
      models: {
        NotHorse: {
          name: "",
          attributes: [],
          navigation_properties: [],
          data_sources: [],
          methods: {},
        },
      },
    };

    // Act
    const result = _cloesceInternal.matchRoute(makeRequest(url), "/api", cidl);

    // Assert
    expect(result.value).toStrictEqual({
      ok: false,
      status: 404,
      message: `Path not found: Unknown model Horse /api/Horse/neigh`,
    });
  });

  test("Router returns 404 on unknown method", () => {
    // Arrange
    const url = "http://foo.com/api/Horse/neigh";
    const cidl = makeCidl({
      notNeigh: {
        name: "",
        is_static: false,
        http_verb: HttpVerb.GET,
        return_type: null,
        parameters: [],
      },
    });

    // Act
    const result = _cloesceInternal.matchRoute(makeRequest(url), "/api", cidl);

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
    const cidl = makeCidl({
      neigh: {
        name: "",
        is_static: false,
        http_verb: HttpVerb.PUT,
        return_type: null,
        parameters: [],
      },
    });

    // Act
    const result = _cloesceInternal.matchRoute(makeRequest(url), "/api", cidl);

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
  const cidl = makeCidl(methods);

  test("Router returns model on static route", () => {
    // Arrange
    const url = "http://foo.com/api/Horse/neigh";

    // Act
    const result = _cloesceInternal.matchRoute(makeRequest(url), "/api", cidl);

    // Assert
    expect(result.value).toStrictEqual({
      modelMeta: cidl.models.Horse,
      methodMeta: cidl.models.Horse.methods.neigh,
      id: null,
    });
  });

  test("Router returns model on instantiated route", () => {
    // Arrange
    const url = "http://foo.com/api/Horse/0/neigh";

    // Act
    const result = _cloesceInternal.matchRoute(makeRequest(url), "/api", cidl);

    // Assert
    expect(result.value).toStrictEqual({
      modelMeta: cidl.models.Horse,
      methodMeta: cidl.models.Horse.methods.neigh,
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
      { models: {} },
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
      { models: {} },
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
      const cidl: MetaCidl = makeCidl({
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
        cidl,
        cidl.models.Horse.methods.neigh,
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
  const input: { typed_value: NamedTypedValue; value: string }[] = [
    {
      typed_value: { name: "id", cidl_type: "Integer", nullable: true },
      value: "1",
    },
    {
      typed_value: { name: "lastName", cidl_type: "Text", nullable: true },
      value: "pumpkin",
    },
    {
      typed_value: { name: "gpa", cidl_type: "Real", nullable: true },
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
        typed_value: { ...i.typed_value, nullable },
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
    const cidl: MetaCidl = makeCidl({
      neigh: {
        name: "neigh",
        is_static: true,
        http_verb: arg.is_get ? HttpVerb.GET : HttpVerb.POST,
        return_type: null,
        parameters: [arg.typed_value],
      },
    });

    // Act
    const result = await _cloesceInternal.validateRequest(
      request,
      cidl,
      cidl.models.Horse.methods.neigh,
      "0",
    );

    // Assert
    expect(result.value).toEqual({
      [arg.typed_value.name]: arg.is_get ? String(arg.value) : arg.value,
    });
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

  const baseCidl: MetaCidl = {
    models: {
      [modelName]: {
        name: modelName,
        attributes: [
          {
            is_primary_key: true,
            value: { name: "id", cidl_type: "Integer", nullable: false },
            foreign_key_reference: null,
          },
          {
            is_primary_key: false,
            value: { name: "name", cidl_type: "Text", nullable: true },
            foreign_key_reference: null,
          },
        ],
        navigation_properties: [
          {
            value: {
              name: "riders",
              cidl_type: { Array: { Model: nestedModelName } },
              nullable: false,
            },
            kind: { OneToMany: { reference: "id" } },
          },
        ],
        data_sources: [],
        methods: {},
      },
      [nestedModelName]: {
        name: nestedModelName,
        attributes: [
          {
            is_primary_key: true,
            value: { name: "id", cidl_type: "Integer", nullable: false },
            foreign_key_reference: null,
          },
          {
            is_primary_key: false,
            value: { name: "nickname", cidl_type: "Text", nullable: true },
            foreign_key_reference: null,
          },
        ],
        navigation_properties: [],
        data_sources: [],
        methods: {},
      },
    },
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
  const createMethodMeta = (overrides: Partial<any> = {}) => ({
    name: "testMethod",
    is_static: true,
    http_verb: HttpVerb.GET,
    return_type: null,
    parameters: [],
    ...overrides,
  });

  const createMockD1 = (): any => ({
    prepare: jest.fn(),
    batch: jest.fn(),
    exec: jest.fn(),
    withSession: jest.fn(),
    dump: jest.fn(),
  });

  test("returns 200 with no data when return_type is null", async () => {
    // Arrange
    const instance = {
      testMethod: jest.fn().mockResolvedValue("ignored"),
    };
    const methodMeta = createMethodMeta({ return_type: null });
    const params = {};
    const d1 = createMockD1();

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      methodMeta,
      params,
      d1,
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
    const methodMeta = createMethodMeta({ return_type: { HttpResult: null } });
    const params = {};
    const d1 = createMockD1();

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      methodMeta,
      params,
      d1,
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
    const methodMeta = createMethodMeta({ return_type: "Text" });
    const params = {};
    const d1 = createMockD1();

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      methodMeta,
      params,
      d1,
    );

    // Assert
    expect(result).toStrictEqual({ ok: true, status: 200, data: "neigh" });
  });

  test("supplies d1 as default param when missing in params", async () => {
    // Arrange
    const d1 = createMockD1();
    const instance = {
      testMethod: jest.fn().mockResolvedValue("used d1"),
    };
    const methodMeta = createMethodMeta({
      return_type: "Text",
      parameters: [{ name: "database" }],
    });
    const params = {}; // missing "database"

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      methodMeta,
      params,
      d1,
    );

    // Assert
    expect(instance.testMethod).toHaveBeenCalledWith(d1);
    expect(result).toStrictEqual({ ok: true, status: 200, data: "used d1" });
  });

  test("returns 500 on thrown Error", async () => {
    // Arrange
    const instance = {
      testMethod: jest.fn().mockImplementation(() => {
        throw new Error("boom");
      }),
    };
    const methodMeta = createMethodMeta({ return_type: "Text" });
    const params = {};
    const d1 = createMockD1();

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      methodMeta,
      params,
      d1,
    );

    // Assert
    expect(result).toStrictEqual({
      ok: false,
      status: 500,
      message: "boom",
    });
  });

  test("returns 500 on thrown non-Error value", async () => {
    // Arrange
    const instance = {
      testMethod: jest.fn().mockImplementation(() => {
        throw "stringError";
      }),
    };
    const methodMeta = createMethodMeta({ return_type: "Text" });
    const params = {};
    const d1 = createMockD1();

    // Act
    const result = await _cloesceInternal.methodDispatch(
      instance,
      methodMeta,
      params,
      d1,
    );

    // Assert
    expect(result).toStrictEqual({
      ok: false,
      status: 500,
      message: "stringError",
    });
  });
});
