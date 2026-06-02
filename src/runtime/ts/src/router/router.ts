import { OrmWasmExports, WasmResource, loadOrmWasm, invokeOrmWasm } from "./wasm.js";
import { Cidl, Model, ApiMethod, CrudKind, DataSource, Field } from "../cidl.js";
import { Either, InternalError } from "../common.js";
import { HttpResult } from "../ui/backend.js";
import { hydrateType } from "./orm.js";
import { crudRoute } from "./crud.js";

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
    public readonly idl: Cidl,
    public readonly wasm: OrmWasmExports,
    public readonly workerUrl: string,
  ) {}

  static async init(idl: Cidl, workerUrl: string) {
    if (this.instance) return;
    const wasmAbi = await loadOrmWasm(idl);
    this.instance = new RuntimeContainer(idl, wasmAbi, workerUrl);
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

  // Maps a model name to its registered namespace object: API method impls,
  // data source impls (under their DS name), and injected dependencies all live here.
  private modelRegistry: Map<string, unknown> = new Map();

  /**
   * Register a model namespace (produced by `Model.impl({...})`) or an injected
   * dependency with the router, making API methods, data source stubs, and
   * injections available for routing.
   */
  public register(model: { readonly tag: string }): this {
    this.modelRegistry.set(model.tag, model);
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
   * Registers middleware for a specific model namespace.
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
   * @param key - The constructor function of the Model.
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
    idl: Cidl,
    wasm: OrmWasmExports,
    di: DependencyContainer,
    workerUrl: string,
  ): Promise<HttpResult<unknown>> {
    // Inject all injectables
    for (const inject of idl.injects) {
      if (this.modelRegistry.has(inject)) {
        di._set({ tag: inject }, this.modelRegistry.get(inject) ?? undefined);
      }
    }

    // Route match
    const routeRes = matchRoute(request, idl, workerUrl, this.modelRegistry);
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
    const validation = await validateRequest(request, wasm, idl, env, route);
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
    const hydrated = await hydrate(this.modelRegistry, di, route, env);
    if (hydrated?.isLeft()) {
      return hydrated.value;
    }

    // Method dispatch
    return await methodDispatch(hydrated?.unwrap(), di, route, params, env);
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
    const { idl, wasm, workerUrl } = RuntimeContainer.get();

    // DI contains explicitly registered injected objects.
    const di = new DependencyContainer();

    try {
      const httpResult = await this.router(request, env, idl, wasm, di, workerUrl);

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
  namespace: string;
  method: ApiMethod;
  getParamValues: Record<string, unknown>;
  impl: ApiImplementation;
  dataSource?: DataSource;
  model: Model;
};

/**
 * @returns 404, 501 or a MatchedRoute
 */
function matchRoute(
  request: Request,
  idl: Cidl,
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

  // instantiated method route format: /{namespace}/{dataSourceGetParams}/method}
  // static method route format: /{namespace}/{method}
  const namespace = parts[0];
  const methodName = parts[parts.length - 1];

  const model = idl.models[namespace];
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

  let dataSource: DataSource | undefined;
  let numGetParams = 0;
  if (method.is_static) {
    if (parts.length !== 2) {
      return notFound(RouterError.UnknownRoute);
    }
  } else {
    dataSource = model.data_sources[method.data_source!];
    numGetParams = dataSource.get.parameters.length;
    if (parts.length !== 2 + numGetParams) {
      return notFound(RouterError.UnknownRoute);
    }
  }

  const userNamespace = registry.get(model.name);
  const impl =
    userNamespace?.[method.name] ??
    (crudRoute(model, method, userNamespace) as ApiImplementation | undefined);
  if (!impl) {
    return notImplemented();
  }

  if (method.is_static) {
    return Either.right({
      namespace,
      method,
      impl,
      getParamValues: {},
      model,
    });
  }

  const getParamValues: Record<string, unknown> = {};
  for (let i = 0; i < numGetParams; i++) {
    const param = dataSource!.get.parameters[i].parameter;
    getParamValues[param.name] = parts[1 + i];
  }

  return Either.right({
    namespace,
    method,
    impl,
    getParamValues,
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
  idl: Cidl,
  env: any,
  route: MatchedRoute,
): Promise<Either<HttpResult, RequestParamMap>> {
  // Error state: any missing parameter, body, or malformed input will exit with 400.
  const invalidRequest = (c: RouterError, reason: string) =>
    exit(400, c, `Invalid Request: ${reason}`);

  // Validate instantiated invocation
  if (!route.method.is_static) {
    const getParams = route.dataSource?.get?.parameters ?? [];
    for (const param of getParams) {
      if (!route.getParamValues[param.parameter.name]) {
        return invalidRequest(
          RouterError.InstantiatedMethodMissingGetParam,
          `Missing get parameter ${param.parameter.name} for instantiated method.`,
        );
      }

      const res = validateField(param.parameter, route.getParamValues[param.parameter.name]);
      if (res.isLeft()) return Either.left(res.unwrapLeft());
      route.getParamValues[param.parameter.name] = res.unwrap();
    }
  }

  const requiredParams = route.method.parameters;

  // Extract all method parameters from the body.
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

  if (route.method.parameters_media === "Octet") {
    // Octet streams are not validated, as they are opaque to Cloesce
    // and the user is expected to handle them manually.
    return Either.right(params);
  }

  // Validate all parameters type.
  for (const p of requiredParams) {
    const res = validateField(p, params[p.name]);
    if (res.isLeft()) return Either.left(res.unwrapLeft());
    const validatedRaw = res.unwrap();
    const hydrated = hydrateType(validatedRaw, p.cidl_type, {
      idl: idl,
      includeTree: null,
      env,
      promises: [],
    });
    params[p.name] = hydrated ?? validatedRaw;
  }

  return Either.right(params);

  function validateField(field: Field, value: unknown): Either<HttpResult, unknown> {
    // Path/query values arrive as raw strings; try JSON-parsing so int/bool/null reach
    // validate_type as their declared type. Falls back to the raw string when the
    // value is a plain string (e.g. for `string`-typed fields).
    let coerced = value;
    if (typeof value === "string") {
      try {
        coerced = JSON.parse(value);
      } catch {
        coerced = value;
      }
    }
    const validateRes = invokeOrmWasm(
      wasm.validate_type,
      [
        WasmResource.fromString(JSON.stringify(field), wasm),
        WasmResource.fromString(JSON.stringify(coerced), wasm),
      ],
      wasm,
    );
    if (validateRes.isLeft()) {
      return invalidRequest(
        RouterError.RequestBodyInvalidParameter,
        `Parameter ${field.name} is invalid: ${validateRes.unwrapLeft()}`,
      );
    }
    return Either.right(JSON.parse(validateRes.unwrap()));
  }
}

/**
 * Hydrates a model instance for method dispatch.
 * @returns 500 or the hydrated instance
 */
async function hydrate(
  registry: Map<string, any>,
  di: DependencyContainer,
  route: MatchedRoute,
  env: any,
): Promise<Either<HttpResult, any>> {
  if (route.method.is_static) {
    // No hydration necessary
    return Either.right(null);
  }

  const meta = route.model!;
  const dsName = route.method.data_source ?? "Default";
  const dataSource: DataSource = meta.data_sources[dsName];

  const hydrationFailed = (e: any) =>
    exit(
      500,
      RouterError.InvalidDatabaseQuery,
      `Error in hydration query: ${e instanceof Error ? e.message : String(e)}`,
    );

  const dsNamespace = registry.get(meta.name)?.[dsName];
  const stub = dsNamespace?.get;
  if (typeof stub !== "function") {
    return exit(
      501,
      RouterError.NotImplemented,
      `${meta.name}.${dsName}.get is declared in the schema but no implementation was provided.`,
    );
  }

  try {
    const args =
      dataSource.get.injected.length > 0
        ? [
            resolveInjectedArgs(di, env, dataSource.get.injected),
            ...Object.values(route.getParamValues),
          ]
        : Object.values(route.getParamValues);
    // Bind `this` to the data source namespace so the generated default impl can
    // reach `this.tree` / `this.getQuery` / `this.listQuery`.
    const result = await stub.apply(dsNamespace, args);

    // Generated default impls return HttpResult; user stubs may return raw values
    // or HttpResults too. Treat any HttpResult as authoritative.
    if (result instanceof HttpResult) {
      if (!result.ok) return Either.left(result);
      if (result.data === null || result.data === undefined) {
        return exit(
          404,
          RouterError.ModelNotFound,
          `Model instance of type ${meta.name} with id: ${JSON.stringify(route.getParamValues)} not found`,
        );
      }
      return Either.right(result.data);
    }

    if (result === null || result === undefined) {
      return exit(
        404,
        RouterError.ModelNotFound,
        `Model instance of type ${meta.name} with id: ${JSON.stringify(route.getParamValues)} not found`,
      );
    }

    return Either.right(result);
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
  env: any,
): Promise<HttpResult<unknown>> {
  const paramArray: any[] = !route.method.is_static ? [obj] : [];

  if (route.method.injected.length > 0) {
    paramArray.push(resolveInjectedArgs(di, env, route.method.injected));
  }

  for (const param of route.method.parameters) {
    paramArray.push(params[param.name]);
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
function resolveInjectedArgs(
  di: DependencyContainer,
  env: any,
  injectedNames: string[],
): Record<string, unknown> {
  const injected: Record<string, unknown> = {};

  for (const name of injectedNames) {
    if (di.has({ tag: name })) {
      injected[name] = di.get({ tag: name });
      continue;
    }

    injected[name] = env?.[name];
    if (injected[name] === undefined) {
      console.warn(`Unable to find injected dependency for ${name}. Leaving as undefined.`);
    }
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
