import {
  OrmWasmExports,
  WasmResource,
  loadOrmWasm,
  invokeOrmWasm,
} from "./wasm.js";
import {
  Cidl,
  Model,
  ApiMethod,
  Service,
  CrudKind,
  DataSource,
  CidlType,
} from "../cidl.js";
import { Either, InternalError } from "../common.js";
import { Orm, KeysOfType, HttpResult } from "../ui/backend.js";
import { hydrateType } from "./orm.js";

const ENV_TAG = "$$env";
export type DependencyKey = { tag: string; } | Function;

/**
 * Dependency injection container, mapping an object type name to an instance of that object.
 *
 * Comes with the WranglerEnv and Request by default.
 */
export class DependencyContainer {
  private container = new Map<string, any>();

  private static tag(key: DependencyKey): string {
    return "tag" in key ? key.tag : key.name;
  }

  set<T>(key: DependencyKey, instance: T) {
    if (this.container.has(DependencyContainer.tag(key))) {
      console.warn(
        `Overwriting existing dependency for key ${DependencyContainer.tag(
          key,
        )}. This may cause unexpected behavior.`,
      );
    }
    this.container.set(DependencyContainer.tag(key), instance);
  }

  get<T>(key: DependencyKey): T | undefined {
    return this.container.get(DependencyContainer.tag(key));
  }

  has(key: DependencyKey): boolean {
    return this.container.has(DependencyContainer.tag(key));
  }
}

/**
 * @internal
 * Singleton instance containing the CIDL, constructor registry, and wasm binary.
 * These values are guaranteed to never change throughout a workers lifetime.
 */
export class RuntimeContainer {
  private static instance: RuntimeContainer | undefined;
  private constructor(
    public readonly ast: Cidl,
    public readonly wasm: OrmWasmExports,
    public readonly workerUrl: string,
  ) { }

  static async init(
    ast: Cidl,
    workerUrl: string,
  ) {
    if (this.instance) return;
    const wasmAbi = await loadOrmWasm(ast);
    this.instance = new RuntimeContainer(
      ast,
      wasmAbi,
      workerUrl,
    );
  }

  static get(): RuntimeContainer {
    return this.instance!;
  }

  /**
   * Disposes the singleton instance. For testing purposes only.
   */
  static dispose() {
    this.instance = undefined;
  }
}

/**
 * @internal
 * Given a request, this represents a map of each body / url  param name to
 * its actual value. Unknown, as the a request can be anything.
 */
export type RequestParamMap = Record<string, unknown>;

export type MiddlewareFn = (
  di: DependencyContainer,
) => Promise<HttpResult | void> | HttpResult | void;

/**
 * Expected states in which the router may exit.
 */
export enum RouterError {
  UnknownPrefix,
  UnknownRoute,
  NotImplemented,
  UnmatchedHttpVerb,
  InstantiatedMethodMissingPrimaryKey,
  InstantiatedMethodMissingKeyParam,
  RequestMissingBody,
  RequestBodyMissingParameters,
  RequestBodyInvalidParameter,
  InvalidDatabaseQuery,
  ModelNotFound,
  UncaughtException,
}

export class CloesceApp {
  public static async init(
    cidl: Cidl,
    workerUrl: string,
  ): Promise<CloesceApp> {
    await RuntimeContainer.init(cidl, workerUrl);
    return new CloesceApp();
  }

  // Maps a model or service name to an instance containing the implementations of its API methods.
  private apiRegistry: Map<string, unknown> = new Map();

  public register(api: { readonly tag: string }): this {
    if (this.apiRegistry.has(api.tag)) {
      console.warn(
        `Overwriting existing API for tag ${api.tag}. This may cause unexpected behavior.`,
      );
    }
    this.apiRegistry.set(api.tag, api);
    return this;
  }

  private onRouteMiddleware: MiddlewareFn[] = [];

  /**
   * Registers middleware than runs on every valid route.
   *
   * @param m - The middleware function to register.
   */
  public onRoute(m: MiddlewareFn) {
    this.onRouteMiddleware.push(m);
  }

  private namespaceMiddleware: Map<{ tag: string }, MiddlewareFn[]> = new Map();

  /**
   * Registers middleware for a specific namespace (Model or Service)
   *
   * Runs before request validation and method middleware, and after services are initialized.
   *
   * @param m - The middleware function to register.
   */
  public onNamespace(key: { tag: string }, m: MiddlewareFn) {
    const existing = this.namespaceMiddleware.get(key);
    if (existing) {
      existing.push(m);
      return;
    }
    this.namespaceMiddleware.set(key, [m]);
  }

  private methodMiddleware: Map<{ tag: string }, Map<string, MiddlewareFn[]>> =
    new Map();

  /**
   * Registers middleware for a specific method on a namespace
   *
   * Runs after namespace middleware and request validation.
   *
   * @param key - The constructor function of the Model or Service.
   * @param method - The method name or CrudKind to register the middleware for.
   * @param m - The middleware function to register.
   */
  public onMethod<T>(
    key: { tag: string },
    method: KeysOfType<T, (...args: any) => any> | CrudKind,
    m: MiddlewareFn,
  ) {
    let classMap = this.methodMiddleware.get(key);
    if (!classMap) {
      classMap = new Map();
      this.methodMiddleware.set(key, classMap);
    }

    let methodArray = classMap.get(method);
    if (!methodArray) {
      methodArray = [];
      classMap.set(method, methodArray);
    }

    methodArray.push(m);
  }

  private async router(
    request: Request,
    env: any,
    ast: Cidl,
    wasm: OrmWasmExports,
    di: DependencyContainer,
    workerUrl: string,
  ): Promise<HttpResult<unknown>> {
    // Initialize services
    // Note: Services are in topological order
    for (const name in ast.services) {
      const serviceMeta: Service = ast.services[name];
      const service: any = {};

      for (const field of serviceMeta.fields) {
        const injected = resolveInjected(di, field.cidl_type);
        service[field.name] = injected;
      }

      // Run init method
      const serviceApi = this.apiRegistry.get(serviceMeta.name) as { init: (self: any) => Promise<HttpResult<void> | void> } | undefined;
      if (serviceApi?.init) {
        const res = await serviceApi.init(service);
        if (res) {
          return res;
        }
      }

      di.set({ tag: serviceMeta.name }, service);
    }

    // Route match
    const routeRes = matchRoute(request, ast, workerUrl, this.apiRegistry);
    if (routeRes.isLeft()) {
      return routeRes.value;
    }
    const route = routeRes.unwrap();

    // Route middleware
    for (const m of this.onRouteMiddleware) {
      const res = await m(di);
      if (res) {
        return res;
      }
    }

    // Namespace middleware
    for (const m of this.namespaceMiddleware.get({ tag: route.namespace }) ??
      []) {
      const res = await m(di);
      if (res) {
        return res;
      }
    }

    // Request validation
    const validation = await validateRequest(
      request,
      wasm,
      ast,
      env,
      route,
    );
    if (validation.isLeft()) {
      return validation.value;
    }
    const params = validation.unwrap();

    // Method middleware
    for (const m of this.methodMiddleware
      .get({ tag: route.namespace })
      ?.get(route.method.name) ?? []) {
      const res = await m(di);
      if (res) {
        return res;
      }
    }

    // Hydration
    const hydrated = await hydrate(di, route, env);
    if (hydrated?.isLeft()) {
      return hydrated.value;
    }

    // Method dispatch
    return await methodDispatch(hydrated?.unwrap(), di, route, params);
  }

  /**
   * Runs the Cloesce Router, handling dependency injection, routing, validation,
   * hydration, and method dispatch.
   *
   * @param request - The incoming Request object.
   * @param env - The Wrangler environment bindings.
   *
   * @returns A Response object representing the result of the request.
   */
  public async run(request: Request, env: any): Promise<Response> {
    const {
      ast,
      wasm,
      workerUrl,
    } = RuntimeContainer.get();

    // DI will always contain the WranglerEnv and Request.
    const di = new DependencyContainer();
    if (ast.wrangler_env) {
      di.set({ tag: ENV_TAG }, env);
    }
    di.set(Request, request);

    try {
      const httpResult = await this.router(
        request,
        env,
        ast,
        wasm,
        di,
        workerUrl,
      );

      // Log any 500 errors
      if (httpResult.status === 500) {
        console.error(
          "A caught error occurred in the Cloesce Router: ",
          httpResult.message,
        );
      }

      return httpResult.toResponse();
    } catch (e: any) {
      let debug: any;
      if (e instanceof Error) {
        debug = {
          name: e.name,
          message: e.message,
          stack: e.stack,
          cause: (e as any).cause,
        };
      } else {
        debug = {
          name: "NonErrorThrown",
          message: typeof e === "string" ? e : JSON.stringify(e),
          stack: undefined,
        };
      }

      const res = HttpResult.fail(500, JSON.stringify(debug));
      console.error(
        "An uncaught error occurred in the Cloesce Router: ",
        debug,
      );
      return res.toResponse();
    }
  }
}

/** @internal */
export type ApiImplementation = (...args: unknown[]) => Promise<unknown> | unknown;

/** @internal */
export type MatchedRoute = {
  kind: "model" | "service";
  namespace: string;
  method: ApiMethod;
  impl: ApiImplementation;
  primaryKeyValues: Record<string, string>;
  keyFields: Record<string, string>;
  model?: Model;
  service?: Service;
};

/**
 * @returns 404, 501 or a MatchedRoute
 */
function matchRoute(
  request: Request,
  ast: Cidl,
  workerUrl: string,
  registry: Map<string, any>,
): Either<HttpResult, MatchedRoute> {
  const url = new URL(request.url);
  const parts = url.pathname.split("/").filter(Boolean);
  const prefix = new URL(workerUrl).pathname.split("/").filter(Boolean);

  // Error state: We expect an exact request format, and expect that the model
  // and are apart of the CIDL
  const notFound = (c: RouterError) => exit(404, c, "Unknown route");
  const notImplemented = () => exit(501, RouterError.NotImplemented, "Not implemented");

  for (const p of prefix) {
    if (parts.shift() !== p) return notFound(RouterError.UnknownPrefix);
  }

  if (parts.length < 2) {
    return notFound(RouterError.UnknownPrefix);
  }

  // Route format: /{namespace}/...{id}/{method}
  const namespace = parts[0];
  const methodName = parts[parts.length - 1];
  const id =
    parts.length > 2
      ? parts.slice(1, parts.length - 1).map(decodeURIComponent)
      : [];

  const model = ast.models[namespace];
  if (model) {
    const method = model.apis.find((a) => a.name === methodName);
    if (!method) return notFound(RouterError.UnknownRoute);

    if (request.method.toLowerCase() !== method.http_verb.toLowerCase()) {
      return notFound(RouterError.UnmatchedHttpVerb);
    }

    const impl = registry.get(model.name)?.[method.name] as ((...args: unknown[]) => Promise<unknown> | unknown) | undefined;
    if (!impl) {
      return notImplemented();
    }

    const numPrimaryKeys = model.primary_columns.length;
    const primaryKeyValues: Record<string, string> = {};

    for (let i = 0; i < numPrimaryKeys; i++) {
      const pkCol = model.primary_columns[i];
      if (i < id.length) {
        primaryKeyValues[pkCol.field.name] = id[i];
      }
    }

    const keyFields = Object.fromEntries(
      id.slice(numPrimaryKeys).map((v, i) => [model.key_fields[i], v]),
    );

    return Either.right({
      kind: "model",
      namespace,
      method,
      model,
      impl,
      primaryKeyValues,
      keyFields,
    });
  }

  const service = ast.services[namespace];
  if (service) {
    const method = service.apis.find((a) => a.name === methodName);
    if (!method || id.length > 0) return notFound(RouterError.UnknownRoute);

    if (request.method.toLowerCase() !== method.http_verb.toLowerCase()) {
      return notFound(RouterError.UnmatchedHttpVerb);
    }

    const impl = registry.get(service.name)?.[method.name] as ((...args: unknown[]) => Promise<unknown> | unknown) | undefined;
    if (!impl) {
      return notImplemented();
    }

    return Either.right({
      kind: "service",
      namespace,
      method,
      service,
      impl,
      primaryKeyValues: {},
      keyFields: {},
    });
  }

  return notFound(RouterError.UnknownRoute);
}

/**
 * Validates the request's body/search params against a ModelMethod
 * @returns 400 or a `RequestParamMap` consisting of each parameters name mapped to its value, and
 * a data source
 */
async function validateRequest(
  request: Request,
  wasm: OrmWasmExports,
  ast: Cidl,
  env: any,
  route: MatchedRoute,
): Promise<Either<HttpResult, RequestParamMap>> {
  // Error state: any missing parameter, body, or malformed input will exit with 400.
  const invalidRequest = (c: RouterError) =>
    exit(400, c, "Invalid Request Body");

  // Validate instantiated model ids
  if (route.kind === "model" && !route.method.is_static) {
    const model = route.model!;

    // Validate all primary key columns are present
    for (const pkCol of model.primary_columns) {
      if (!(pkCol.field.name in route.primaryKeyValues)) {
        return invalidRequest(RouterError.InstantiatedMethodMissingPrimaryKey);
      }
    }

    if (model.key_fields.length !== Object.keys(route.keyFields).length) {
      return invalidRequest(RouterError.InstantiatedMethodMissingKeyParam);
    }

    for (const keyParam of model.key_fields) {
      if (!(keyParam in route.keyFields)) {
        return invalidRequest(RouterError.InstantiatedMethodMissingKeyParam);
      }
    }
  }

  // Filter out injected parameters
  const requiredParams = route.method.parameters.filter(
    (p) => !(typeof p.cidl_type === "object" && "Inject" in p.cidl_type),
  );

  // Extract all method parameters from the body
  const url = new URL(request.url);
  let params: RequestParamMap = Object.fromEntries(url.searchParams.entries());
  if (route.method.http_verb !== "Get") {
    try {
      switch (route.method.parameters_media) {
        case "Json": {
          const body = await request.json();
          params = { ...params, ...body };
          break;
        }
        case "Octet": {
          // Octet streams are verified by Cloesce to have
          // one Stream type
          const streamParam = requiredParams.find(
            (p) => typeof p.cidl_type === "string" && p.cidl_type === "Stream",
          )!;

          params[streamParam.name] = request.body;
          break;
        }
        default: {
          throw new InternalError("not implemented");
        }
      }
    } catch {
      return invalidRequest(RouterError.RequestMissingBody);
    }
  }

  if (!requiredParams.every((p) => p.name in params)) {
    return invalidRequest(RouterError.RequestBodyMissingParameters);
  }

  // Validate all parameters type. Octet streams need no validation.
  if (route.method.parameters_media !== "Octet") {
    for (const p of requiredParams) {
      const validateRes = invokeOrmWasm(
        wasm.validate_type,
        [
          WasmResource.fromString(JSON.stringify(p.cidl_type), wasm),
          WasmResource.fromString(JSON.stringify(params[p.name]), wasm),
        ],
        wasm,
      );

      if (validateRes.isLeft()) {
        return invalidRequest(RouterError.RequestBodyInvalidParameter);
      }

      const validatedRaw = JSON.parse(validateRes.unwrap());
      const hydrated = hydrateType(validatedRaw, p.cidl_type, {
        ast,
        includeTree: null,
        keyFields: {},
        env,
        promises: [],
      });
      const validatedParam = hydrated ?? validatedRaw;
      params[p.name] = validatedParam;
    }
  }

  return Either.right(params);
}

/**
 * Hydrates a model or service instance for method dispatch.
 * @returns 500 or the hydrated instance
 */
async function hydrate(
  di: DependencyContainer,
  route: MatchedRoute,
  env: any,
): Promise<Either<HttpResult, any>> {
  if (route.method.is_static) {
    // No hydration necessary
    return Either.right(null);
  }

  if (route.kind === "service") {
    // Fetch the existing service instance from DI
    return Either.right(di.get({ tag: route.namespace }));
  }

  const meta = route.model!;
  const dataSource: DataSource = meta.data_sources[route.method.data_source ?? "Default"];
  const orm = Orm.fromEnv(env);

  // Error state: If some outside force tweaked the database schema, the query may fail.
  // Otherwise, this indicates a bug in the compiler or runtime.
  const malformedQuery = (e: any) =>
    exit(
      500,
      RouterError.InvalidDatabaseQuery,
      `Error in hydration query: ${e instanceof Error ? e.message : String(e)}`,
    );

  try {
    let result = null;
    if (dataSource.get === undefined) {
      // Must be a KV or R2 based model
      result = await orm.getCustom(meta, null, {}, route.keyFields, dataSource.include as any);
    } else {
      const query = dataSource.get(...Object.values(route.primaryKeyValues));
      result = await orm.getQuery(meta, query, dataSource.include as any, route.keyFields);
    }

    // Result will only be null if the record does not exist for a D1 query.
    if (result === null) {
      const pkValues = Object.values(route.primaryKeyValues).join(", ");
      return exit(
        404,
        RouterError.ModelNotFound,
        `Model instance of type ${meta.name} with primary key (${pkValues}) not found`,
      );
    }

    return Either.right(result);
  } catch (e) {
    return malformedQuery(JSON.stringify(e));
  }
}

/**
 * Calls a method on a model given a list of parameters.
 * @returns 500 on an uncaught client error, 200 with a result body on success
 */
async function methodDispatch(
  obj: any,
  di: DependencyContainer,
  route: MatchedRoute,
  params: Record<string, unknown>,
): Promise<HttpResult<unknown>> {
  const paramArray: any[] = route.method.is_static ? [] : [obj];
  for (const param of route.method.parameters) {
    if (param.name in params) {
      paramArray.push(params[param.name]);
      continue;
    }

    // Assume injected parameter
    const injected = resolveInjected(
      di,
      param.cidl_type,
    );
    paramArray.push(injected);
  }

  const wrapResult = (res: any): HttpResult => {
    const rt = route.method.return_type;
    const httpResult: HttpResult<unknown> =
      typeof rt === "object" && rt !== null && "HttpResult" in rt
        ? res
        : HttpResult.ok(200, res);
    return httpResult.setMediaType(route.method.return_media);
  };

  try {
    const res = await route.impl(...paramArray);
    return wrapResult(res);
  } catch (e) {
    // Error state: Client code threw an uncaught exception.
    return exit(
      500,
      RouterError.UncaughtException,
      `Uncaught exception in method dispatch: ${e instanceof Error ? e.message : String(e)}`,
    ).unwrapLeft();
  }
}

function exit(
  status: number,
  state: RouterError,
  message: string,
  debugMessage: string = "",
): Either<HttpResult<void>, never> {
  return Either.left(
    HttpResult.fail(status, `${message} (ErrorCode: ${state}${debugMessage})`),
  );
}

/**
 * Finds an injected dependency from the DI container.
 * @returns The injected dependency, or undefined if not found.
 */
function resolveInjected(di: DependencyContainer, ty: CidlType): any | undefined {
  let tag = null;
  if (typeof ty === "object" && "Inject" in ty) {
    tag = ty.Inject.name;
  } else if (typeof ty === "string" && ty === "Env") {
    tag = ENV_TAG;
  } else {
    throw new InternalError(
      `Invalid injected type: ${JSON.stringify(ty)}. Expected an Inject type or Env.`,
    );
  }

  const injected = di.get({ tag });
  if (injected === undefined) {
    console.warn(
      `Unable to find injected dependency for ${tag}. Leaving as undefined.`,
    );
  }
  return injected;
}

/**
 * @internal
 * Exported for testing purposes only.
 */
export const _cloesceInternal = {
  matchRoute,
  validateRequest,
  methodDispatch,
  RuntimeContainer,
};
