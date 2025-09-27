import { cloesceStates } from "../src/cloesce";
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
    method ? { method, body: body && JSON.stringify(body) } : undefined
  );

describe("Router Error States", () => {
  test("Router returns 404 on route missing model, method", () => {
    // Arrange
    const url = "http://foo.com/api";

    // Act
    const result = cloesceStates.matchRoute(makeRequest(url), "/api", {
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
    const result = cloesceStates.matchRoute(makeRequest(url), "/api", cidl);

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
    const result = cloesceStates.matchRoute(makeRequest(url), "/api", cidl);

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
    const result = cloesceStates.matchRoute(makeRequest(url), "/api", cidl);

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
    const result = cloesceStates.matchRoute(makeRequest(url), "/api", cidl);

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
    const result = cloesceStates.matchRoute(makeRequest(url), "/api", cidl);

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
    const result = await cloesceStates.validateRequest(
      request,
      { models: {} },
      {
        name: "",
        is_static: false,
        http_verb: HttpVerb.GET,
        return_type: null,
        parameters: [],
      },
      null
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
    const result = await cloesceStates.validateRequest(
      request,
      { models: {} },
      {
        name: "",
        is_static: true,
        http_verb: HttpVerb.PATCH,
        return_type: null,
        parameters: [],
      },
      null
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
      const result = await cloesceStates.validateRequest(
        request,
        cidl,
        cidl.models.Horse.methods.neigh,
        "0"
      );

      // Assert
      expect(result.value).toStrictEqual({
        ok: false,
        status: 400,
        message: `Invalid Request Body: ${message}.`,
      });
    }
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
      }))
    )
  );

  test.each(expanded)("input is accepted %#", async (arg) => {
    // Arrange
    const url = arg.is_get
      ? `http://foo.com/api/Horse/neigh?${arg.typed_value.name}=${arg.value}`
      : "http://foo.com/api/Horse/neigh";
    const request = makeRequest(
      url,
      arg.is_get ? undefined : "POST",
      arg.is_get ? undefined : { [arg.typed_value.name]: arg.value }
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
    const result = await cloesceStates.validateRequest(
      request,
      cidl,
      cidl.models.Horse.methods.neigh,
      "0"
    );

    // Assert
    expect(result.value).toEqual({
      [arg.typed_value.name]: arg.is_get ? String(arg.value) : arg.value,
    });
  });
});
