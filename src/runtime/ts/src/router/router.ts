import { OrmWasmExports, WasmResource, loadOrmWasm, invokeOrmWasm } from "./wasm.js";
import { Cidl, Model, ApiMethod, Service, CrudKind, DataSource, CidlType } from "../cidl.js";
import { CloesceError, CloesceResult, Either, InternalError } from "../common.js";
import { HttpResult } from "../ui/backend.js";
import { hydrateType } from "./orm.js";
import { crudRoute } from "./crud.js";

const ENV_TAG = "$$env";
export type DependencyKey = { tag: string };

/**
 * Dependency injection container, mapping an object type name to an instance of that object.
 *
 * Comes with the Wrangler environment pre-injected
 */
export class DependencyContainer {
  private container = new Map<string, any>();

  /** @internal */
  _set<T>(key: DependencyKey, instance: T) {
    if (this.container.has(key.tag)) {
      console.warn(
        `Overwriting existing dependency for key ${key.tag}. This may cause unexpected behavior.`,
      );
    }
    this.container.set(key.tag, instance);
  }

  set(value: { tag: string }) {
    this._set({ tag: value.tag }, value);
  }

  get<T>(key: DependencyKey): T | undefined {
    return this.container.get(key.tag);
  }

  has(key: DependencyKey): boolean {
    return this.container.has(key.tag);
  }
}

/**
 * @internal
 * Singleton instance containing the CIDL and and wasm binary.
 * These values are guaranteed to never change throughout a workers lifetime.
 */
export class RuntimeContainer {
  private static instance: RuntimeContainer | undefined;
  private constructor(
    public readonly ast: Cidl,
    public readonly wasm: OrmWasmExports,
    public readonly workerUrl: string,
  ) {}

  static async init(ast: Cidl, workerUrl: string) {
    if (this.instance) return;
    const wasmAbi = await loadOrmWasm(ast);
    this.instance = new RuntimeContainer(ast, wasmAbi, workerUrl);
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
 * States in which the router may exit.
 */
export enum RouterError {
  UnknownPrefix,
  UnknownRoute,
  NotImplemented,
  UnmatchedHttpVerb,
  InstantiatedMethodMissingGetParam,
  InstantiatedMethodMissingKeyParam,
  RequestMissingBody,
  RequestBodyMissingParameters,
  RequestBodyInvalidParameter,
  InvalidDatabaseQuery,
  ModelNotFound,
  UncaughtException,
}

export class CloesceApp {
  public static async init(cidl: Cidl, workerUrl: string): Promise<CloesceApp> {
    await RuntimeContainer.init(cidl, workerUrl);
    return new CloesceApp();
  }

  // Maps a model or service name to an instance containing the implementations of its API methods.
  // Additionally, contains injected dependencies, mapped to their instance.
  private apiRegistry: Map<string, unknown> = new Map();

  /**
   * Register an API implementation or Injected dependency with the router,
   * making it available for routing and injection, respectively.
   */
  public register(api: { readonly tag: string }): this {
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

  private namespaceMiddleware: Map<string, MiddlewareFn[]> = new Map();

  /**
   * Registers middleware for a specific namespace (Model or Service)
   *
   * Runs before request validation and method middleware.
   *
   * @param m - The middleware function to register.
   */
  public onNamespace(tag: string, m: MiddlewareFn) {
    const existing = this.namespaceMiddleware.get(tag);
    if (existing) {
      existing.push(m);
      return;
    }
    this.namespaceMiddleware.set(tag, [m]);
  }

  private methodMiddleware: Map<string, Map<string, MiddlewareFn[]>> = new Map();

  /**
   * Registers middleware for a specific method on a namespace
   *
   * Runs after namespace middleware and request validation.
   *
   * @param key - The constructor function of the Model or Service.
   * @param method - The method name or CrudKind to register the middleware for.
   * @param m - The middleware function to register.
   */
  public onMethod(tag: string, method: string | CrudKind, m: MiddlewareFn) {
    let classMap = this.methodMiddleware.get(tag);
    if (!classMap) {
      classMap = new Map();
      this.methodMiddleware.set(tag, classMap);
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
    // Inject all injectables
    for (const inject of ast.injects) {
      if (this.apiRegistry.has(inject)) {
        di._set({ tag: inject }, this.apiRegistry.get(inject) ?? undefined);
      }
    }

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
      type ServiceInit = {
        init?: (self: any) => Promise<HttpResult<void> | void>;
      };
      const serviceApi = this.apiRegistry.get(serviceMeta.name) as ServiceInit | undefined;
      if (serviceApi?.init) {
        const res = await serviceApi.init(service);
        if (res) {
          return res;
        }
      }

      di._set({ tag: serviceMeta.name }, service);
    }

    // Route match
    const routeRes = matchRoute(request, ast, workerUrl, this.apiRegistry, env);
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
    for (const m of this.namespaceMiddleware.get(route.namespace) ?? []) {
      const res = await m(di);
      if (res) {
        return res;
      }
    }

    // Request validation
    const validation = await validateRequest(request, wasm, ast, env, route);
    if (validation.isLeft()) {
      return validation.value;
    }
    const params = validation.unwrap();

    // Method middleware
    for (const m of this.methodMiddleware.get(route.namespace)?.get(route.method.name) ?? []) {
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
    const { ast, wasm, workerUrl } = RuntimeContainer.get();

    // DI will always contain the WranglerEnv
    const di = new DependencyContainer();
    if (ast.wrangler_env) {
      di._set({ tag: ENV_TAG }, env);
    }

    try {
      const httpResult = await this.router(request, env, ast, wasm, di, workerUrl);

      // Log any 500 errors
      if (httpResult.status === 500) {
        console.error("A caught error occurred in the Cloesce Router: ", httpResult.message);
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
      console.error("An uncaught error occurred in the Cloesce Router: ", debug);
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
  getParamValues: Record<string, string>;
  keyFields: Record<string, string>;
  impl: ApiImplementation;
  dataSource?: DataSource;
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
  env: any,
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

  // instantiated method route format: /{namespace}/{dataSourceGetParams}/{keyFields}/{method}
  // static/service method route format: /{namespace}/{method}
  const namespace = parts[0];
  const methodName = parts[parts.length - 1];

  const service = ast.services[namespace];
  if (service) {
    if (parts.length !== 2) {
      return notFound(RouterError.UnknownRoute);
    }

    const method = service.apis.find((a) => a.name === methodName);
    if (!method) {
      return notFound(RouterError.UnknownRoute);
    }

    if (request.method.toLowerCase() !== method.http_verb.toLowerCase()) {
      return notFound(RouterError.UnmatchedHttpVerb);
    }

    const impl = registry.get(service.name)?.[method.name] as ApiImplementation | undefined;
    if (!impl) {
      return notImplemented();
    }

    return Either.right({
      kind: "service",
      namespace,
      method,
      impl,
      getParamValues: {},
      keyFields: {},
      service,
    });
  }

  const model = ast.models[namespace];
  if (!model) {
    return notFound(RouterError.UnknownRoute);
  }

  const method = model.apis.find((a) => a.name === methodName);
  if (!method) {
    return notFound(RouterError.UnknownRoute);
  }
  if (request.method.toLowerCase() !== method.http_verb.toLowerCase()) {
    return notFound(RouterError.UnmatchedHttpVerb);
  }

  let impl =
    registry.get(model.name)?.[method.name] ??
    (crudRoute(model, method, env) as ApiImplementation | undefined);
  if (!impl) {
    return notImplemented();
  }

  if (method.is_static) {
    if (parts.length !== 2) {
      return notFound(RouterError.UnknownRoute);
    }

    return Either.right({
      kind: "model",
      namespace,
      method,
      impl,
      getParamValues: {},
      keyFields: {},
      model,
    });
  }

  // With N data source get params and M key fields, the id portion of the route
  // should be N+M segments long, with the first N segments in the order of the data source get parameters
  const dataSource = model.data_sources[method.data_source!];
  const numGetParams = dataSource.get ? dataSource.get.parameters.length : 0;
  const numKeyFields = model.key_fields.length;
  if (parts.length !== 2 + numGetParams + numKeyFields) {
    return notFound(RouterError.UnknownRoute);
  }

  const getParamValues: Record<string, string> = {};
  for (let i = 0; i < numGetParams; i++) {
    const param = dataSource.get!.parameters[i];
    getParamValues[param.name] = parts[1 + i];
  }

  const keyFields: Record<string, string> = {};
  for (let i = 0; i < numKeyFields; i++) {
    const field = model.key_fields[i];
    keyFields[field.name] = parts[1 + numGetParams + i];
  }

  return Either.right({
    kind: "model",
    namespace,
    method,
    impl,
    getParamValues,
    keyFields,
    dataSource,
    model,
  });
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
  const invalidRequest = (c: RouterError, reason: string) =>
    exit(400, c, `Invalid Request: ${reason}`);

  // Validate instantiated invocation
  if (route.kind === "model" && !route.method.is_static) {
    const model = route.model!;

    // Validate all data source get parameters are present
    for (const field of route.dataSource?.get?.parameters ?? []) {
      if (!(field.name in route.getParamValues)) {
        return invalidRequest(
          RouterError.InstantiatedMethodMissingGetParam,
          `Missing data source get parameter ${field.name}`,
        );
      }
    }

    for (const field of model.key_fields) {
      if (!(field.name in route.keyFields)) {
        return invalidRequest(
          RouterError.InstantiatedMethodMissingKeyParam,
          `Missing key field ${field.name}`,
        );
      }
    }
  }

  // Filter out injected parameters
  const requiredParams = route.method.parameters.filter(
    (p) => p.cidl_type !== "Env" && !(typeof p.cidl_type === "object" && "Inject" in p.cidl_type),
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
      return invalidRequest(RouterError.RequestMissingBody, "Request body is missing or malformed");
    }
  }

  if (!requiredParams.every((p) => p.name in params)) {
    return invalidRequest(
      RouterError.RequestBodyMissingParameters,
      "One or more required parameters are missing",
    );
  }

  // Validate all parameters type. Octet streams need no validation.
  if (route.method.parameters_media !== "Octet") {
    for (const p of requiredParams) {
      const validateRes = invokeOrmWasm(
        wasm.validate_type,
        [
          WasmResource.fromString(JSON.stringify(p), wasm), // field metadata
          WasmResource.fromString(JSON.stringify(params[p.name]), wasm), // value
        ],
        wasm,
      );

      if (validateRes.isLeft()) {
        const message = validateRes.unwrapLeft();
        return invalidRequest(
          RouterError.RequestBodyInvalidParameter,
          `Parameter ${p.name} is invalid: ${message}`,
        );
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

  // Error state: If some outside force tweaked the database schema, or some outage caused the
  // data store to return an error, hydration may fail.
  const hydrationFailed = (e: any) =>
    exit(
      500,
      RouterError.InvalidDatabaseQuery,
      `Error in hydration query: ${e instanceof Error ? e.message : String(e)}`,
    );

  try {
    let result = await dataSource.gen.get(
      env,
      ...Object.values(route.getParamValues),
      ...Object.values(route.keyFields),
    );

    if (result.errors.length > 0) {
      return hydrationFailed(CloesceError.displayErrors(result as CloesceResult<never>));
    }

    // Result will only be null if the record does not exist for a D1 query
    // (KV or R2 based models will just be empty, as that is a valid state).
    if (result.value === null) {
      return exit(
        404,
        RouterError.ModelNotFound,
        `Model instance of type ${meta.name} with id: ${JSON.stringify(route.getParamValues)} not found`,
      );
    }

    return Either.right(result.value);
  } catch (e) {
    return hydrationFailed(JSON.stringify(e));
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
    const injected = resolveInjected(di, param.cidl_type);
    paramArray.push(injected);
  }

  const wrapResult = (res: any): HttpResult => {
    const httpResult = res instanceof HttpResult ? res : HttpResult.ok(200, res);
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
  return Either.left(HttpResult.fail(status, `${message} (ErrorCode: ${state}${debugMessage})`));
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
    return undefined;
  }

  const injected = di.get({ tag });
  if (injected === undefined) {
    console.warn(`Unable to find injected dependency for ${tag}. Leaving as undefined.`);
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
