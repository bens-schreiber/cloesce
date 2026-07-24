import { OrmWasmExports, WasmResource, loadOrmWasm, invokeOrmWasm } from "./wasm.js";
import { Cidl, Model, ApiMethod, DataSource, Field, ENV_DURABLE_TARGET_KEY } from "../cidl.js";
import { Either, InternalError } from "../common.js";
import { HttpResult } from "../ui/backend.js";
import { hydrateType } from "./orm.js";
import { crudRoute } from "./crud.js";
import { sourceStore } from "../app/store.js";
import { DurableObjectNamespace } from "@cloudflare/workers-types";

/**
 * @internal
 * Singleton instance containing the CIDL and and wasm binary.
 * These values are guaranteed to never change throughout a Workers lifetime.
 */
export class RuntimeContainer {
  private static instance: RuntimeContainer | undefined;
  private constructor(
    public readonly idl: Cidl,
    public readonly wasm: OrmWasmExports,
  ) {}

  static async init(idl: Cidl) {
    if (this.instance) return;
    const wasmAbi = await loadOrmWasm(idl);
    this.instance = new RuntimeContainer(idl, wasmAbi);
  }

  static get(): RuntimeContainer {
    if (!this.instance) {
      throw new InternalError(
        `Cloesce RuntimeContainer accessed before initialization. 
        Call CloesceApp.forceLoad() or CloesceApp.run() before accessing the RuntimeContainer.`,
      );
    }
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
 * Given a request, this represents a map of each body / url  param name to
 * its actual value. Unknown, as the a request can be anything.
 */
type RequestParams = Record<string, unknown>;

/**
 * States in which the router may exit.
 */
enum RouterError {
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

/**
 * @internal
 * Runs the Cloesce router pipeline for one request: route match, validation,
 * self hydration, and method dispatch.
 *
 * @param registry Maps a model name to its registered implementation module
 *   (route consts + custom / override data-source consts).
 */
export async function router(
  request: Request,
  idl: Cidl,
  workerUrl: string,
  env: any,
  registry: Map<string, any>,
  durableContext: unknown,
): Promise<Response> {
  await RuntimeContainer.init(idl);

  try {
    const result = await route(request, idl, workerUrl, env, registry, durableContext);

    if (result instanceof Response) {
      // A forwarded Durable Object response is passed through unchanged.
      return result;
    }

    if (result.status === 500) {
      console.error("A caught error occurred in the Cloesce Router: ", result.message);
    }

    return result.toResponse();
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

async function route(
  request: Request,
  idl: Cidl,
  workerUrl: string,
  env: any,
  registry: Map<string, any>,
  durableContext: unknown,
): Promise<HttpResult<unknown> | Response> {
  const { wasm } = RuntimeContainer.get();

  const routeRes = matchRoute(request, idl, workerUrl);
  if (routeRes.isLeft()) {
    return routeRes.value;
  }
  const route = routeRes.unwrap();

  route.impl = resolveImpl(route, env, registry) ?? undefined;
  if (!route.impl && !route.forward) {
    return HttpResult.fail(501, "Not implemented");
  }

  const forwardRequest = route.forward ? request.clone() : undefined;

  const validation = await validateRequest(request, wasm, idl, env, route);
  if (validation.isLeft()) {
    return validation.value;
  }
  const params = validation.unwrap();

  if (forwardRequest) {
    return await forward(route, env, params, forwardRequest);
  }

  const hydrated = await hydrateSelf(route, env);
  if (hydrated?.isLeft()) {
    return hydrated.value;
  }

  return await methodDispatch(route.impl!, hydrated?.unwrap(), route, params, env, durableContext);
}

/**
 * Resolve the implementation for a route: a `$`-prefixed CRUD route dispatches to the
 * model's env store; any other route is a plain exported const in the registered module.
 */
function resolveImpl(
  route: MatchedRoute,
  env: any,
  registry: Map<string, any>,
): ApiImplementation | undefined {
  if (route.method.name.startsWith("$")) {
    return crudRoute(route.model, route.method, env) ?? undefined;
  }
  return registry.get(route.model.name)?.[route.method.name];
}

/** @internal */
export type ApiImplementation = (...args: unknown[]) => Promise<unknown> | unknown;

/** @internal */
export type MatchedRoute = {
  namespace: string;
  method: ApiMethod;
  getParamValues: Record<string, unknown>;
  forward: boolean;
  impl?: ApiImplementation;
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
): Either<HttpResult, MatchedRoute> {
  const url = new URL(request.url);
  const parts = url.pathname.split("/").filter(Boolean);
  const prefix = new URL(workerUrl).pathname.split("/").filter(Boolean);

  // Error state: We expect an exact request format, and expect that the model
  // and are apart of the CIDL
  const notFound = (c: RouterError) => exit(404, c, "Unknown route");

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

  // Cloesce will mark requests that were forwarded to this router
  // with the `cloesce-forwarded` header.
  //
  // If this header is present, we know that the request cannot be forwarded again
  // and the impl must be present on this router.
  //
  // If the header is not present, but the method has a durable target, we can mark
  // this route as one that should be forwarded.
  const forwardedHeader = request.headers.get("cloesce-forwarded");
  const hasDurableTarget = method.durable_target != null;
  const forward = forwardedHeader === null && hasDurableTarget;

  if (method.is_static) {
    return Either.right({
      namespace,
      method,
      forward,
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
    forward,
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
): Promise<Either<HttpResult, RequestParams>> {
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

  // Extract all parameters
  const url = new URL(request.url);
  let params: RequestParams = Object.fromEntries(url.searchParams.entries());

  // A JSON body is only present when at least one parameter is Body-sourced.
  const hasBodyParams = requiredParams.some((p) => p.source === "Body");

  if (
    route.method.http_verb !== "Get" &&
    (hasBodyParams || route.method.parameters_media === "Octet")
  ) {
    try {
      switch (route.method.parameters_media) {
        case "Json": {
          const body = await request.json<RequestParams>();
          params = { ...params, ...body };
          break;
        }
        case "Octet": {
          // Octet streams are verified by Cloesce to have
          // one Stream type
          const streamParam = requiredParams.find(
            (p) => typeof p.field.cidl_type === "string" && p.field.cidl_type === "Stream",
          )!;

          params[streamParam.field.name] = request.body;
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

  for (const p of requiredParams) {
    if (p.source !== "Header") continue;
    const raw = readHeader(request, p.field.name);
    if (raw !== null) {
      params[p.field.name] = raw;
    }
  }

  if (!requiredParams.every((p) => p.field.name in params)) {
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
    const res = validateField(p.field, params[p.field.name]);
    if (res.isLeft()) return Either.left(res.unwrapLeft());
    const validatedRaw = res.unwrap();
    const hydrated = hydrateType(validatedRaw, p.field.cidl_type, {
      idl: idl,
      includeTree: null,
      env,
    });
    params[p.field.name] = hydrated ?? validatedRaw;
  }

  return Either.right(params);

  function validateField(field: Field, value: unknown): Either<HttpResult, unknown> {
    const validateRes = invokeOrmWasm(
      wasm.validate_type,
      [
        WasmResource.fromString(JSON.stringify(field), wasm),
        WasmResource.fromString(JSON.stringify(value), wasm),
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

/** A header may come in the form `Header_Name` or `Header-Name`, matching either here. */
function readHeader(request: Request, name: string): string | null {
  return request.headers.get(name) ?? request.headers.get(name.replaceAll("_", "-"));
}

/**
 * Forwards a request to a Durable Object instance
 */
async function forward(
  route: MatchedRoute,
  env: any,
  params: Record<string, unknown>,
  request: Request,
): Promise<Response> {
  const target = route.method.durable_target!;

  // Shard values that locate the DO are top-level method parameters: for an
  // instantiated (`self`) method they arrive as route/path params, for a static
  // method they arrive in the body/search params.
  const name = [
    target.binding,
    ...target.shard_args.map((name) => String(route.getParamValues[name] ?? params[name])),
  ].join("/");

  const namespace = env[target.binding] as DurableObjectNamespace;
  const stub = namespace.get(namespace.idFromName(name));
  const forwarded = new Request(request);
  forwarded.headers.set("cloesce-forwarded", "true");
  return await stub.fetch(forwarded);
}

/**
 * Loads `self` for an instance route from the model's env store (default or `[source X]`
 * source), returning the hydrated value for method dispatch.
 * @returns 404/500/501 on the left, or the hydrated instance on the right.
 */
async function hydrateSelf(route: MatchedRoute, env: any): Promise<Either<HttpResult, any>> {
  if (route.method.is_static) {
    // No hydration necessary
    return Either.right(null);
  }

  const meta = route.model;
  const dsName = route.method.data_source ?? "Default";
  const store = sourceStore(env, meta, dsName);
  const getVerb = store?.get;
  if (typeof getVerb !== "function") {
    return exit(
      501,
      RouterError.NotImplemented,
      `${meta.name}.${dsName}.get is declared in the schema but no implementation was provided.`,
    );
  }

  const notFound = () =>
    exit(
      404,
      RouterError.ModelNotFound,
      `Model instance of type ${meta.name} with id: ${JSON.stringify(route.getParamValues)} not found`,
    );

  try {
    const ds = meta.data_sources[dsName];
    const args = ds.get.parameters.map((p) => route.getParamValues[p.parameter.name]);
    const result = await getVerb(...args);

    if (result instanceof HttpResult) {
      if (!result.ok) return Either.left(result);
      if (result.data === null || result.data === undefined) return notFound();
      return Either.right(result.data);
    }

    if (result === null || result === undefined) return notFound();
    return Either.right(result);
  } catch (e) {
    return exit(
      500,
      RouterError.InvalidDatabaseQuery,
      `Error in hydration query: ${e instanceof Error ? e.message : String(e)}`,
    );
  }
}

/**
 * Calls a route implementation with the conventional argument order:
 * `(self?, env?, ...params)`. `self` is present for instance routes; the full upgraded
 * `env` is present whenever the route injects a binding or runs in a Durable Object.
 * @returns 500 on an uncaught client error, 200 with a result body on success
 */
async function methodDispatch(
  impl: ApiImplementation,
  obj: any,
  route: MatchedRoute,
  params: Record<string, unknown>,
  env: any,
  durableContext: unknown,
): Promise<HttpResult<unknown>> {
  const paramArray: any[] = !route.method.is_static ? [obj] : [];

  // CRUD (`$verb`) routes dispatch to an env-bound store verb that closes over `env`;
  // they never receive `env` as an argument even though the schema records their bindings.
  const isCrud = route.method.name.startsWith("$");
  if (!isCrud && (route.method.injected.length > 0 || route.method.durable_target != null)) {
    // A durable route runs inside its DO; surface that context under the well-known key.
    if (route.method.durable_target != null) {
      env[ENV_DURABLE_TARGET_KEY] = durableContext;
    }
    paramArray.push(env);
  }

  for (const param of route.method.parameters) {
    paramArray.push(params[param.field.name]);
  }

  const wrapResult = (res: any): HttpResult => {
    const httpResult = res instanceof HttpResult ? res : HttpResult.ok(200, res);
    return httpResult.setMediaType(route.method.return_media);
  };

  try {
    const res = await impl(...paramArray);
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
 * @internal
 * Exported for testing purposes only.
 */
export const _cloesceInternal = {
  matchRoute,
  validateRequest,
  methodDispatch,
  RuntimeContainer,
  RouterError,
};
