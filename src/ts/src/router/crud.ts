import { Orm, HttpResult } from "../ui/backend.js";
import { DataSource } from "./orm.js";
import { findDataSource, RuntimeContainer } from "./router.js";

/**
 * @internal
 *
 * Wraps an object in a Proxy that will intercept non-overriden CRUD methods,
 * calling a default implementation.
 */
export function proxyCrud(obj: any, ctor: any, env: any) {
  return new Proxy(obj, {
    get(target, method) {
      // If the instance defines the method, always use it (override allowed)
      const value = Reflect.get(target, method);
      if (typeof value === "function") {
        return value.bind(target);
      }

      // Fallback to CRUD methods
      if (method === "SAVE") {
        return (body: object, ds: string) => upsert(ctor, body, ds, env);
      }

      if (method === "LIST") {
        return (ds: string) => list(ctor, ds, env);
      }

      if (method === "GET") {
        return (...args: any[]) => _get(ctor, args, env);
      }

      return value;
    },
  });
}

async function upsert(
  ctor: any,
  body: object,
  dataSourceRef: string,
  env: any,
): Promise<HttpResult<unknown>> {
  const dataSource = findDataSource(ctor, dataSourceRef);
  const orm = Orm.fromEnv(env);

  // Upsert
  const result: any | null = await orm.upsert(ctor, body, dataSource);
  return !result ? HttpResult.fail(404) : HttpResult.ok(200, result);
}

async function _get(
  ctor: any,
  args: any[],
  env: any,
): Promise<HttpResult<unknown>> {
  const { ast } = RuntimeContainer.get();
  const model = ast.models[ctor.name];

  const getArgs: {
    id?: any;
    keyParams?: Record<string, any>;
    include?: DataSource<any>;
  } = {};

  let argIndex = 0;
  if (model.primary_key) {
    // If there is a primary key, the first argument is the primary key.
    getArgs.id = args[argIndex++];
  }

  if (model.key_params.length > 0) {
    // All key params come after the primary key.
    // Order is guaranteed by the compiler.
    getArgs.keyParams = {};
    for (const keyParam of model.key_params) {
      getArgs.keyParams[keyParam] = args[argIndex++];
    }
  }

  // The last argument is always the data source.
  getArgs.include = findDataSource(ctor, args[argIndex]);

  const orm = Orm.fromEnv(env);
  const result: any | null = await orm.get(ctor, getArgs);
  return !result ? HttpResult.fail(404) : HttpResult.ok(200, result);
}

async function list(
  ctor: any,
  dataSourceRef: string,
  env: any,
): Promise<HttpResult<unknown>> {
  const dataSource = findDataSource(ctor, dataSourceRef);
  const orm = Orm.fromEnv(env);

  const result: any[] = await orm.list(ctor, dataSource);
  return HttpResult.ok(200, result);
}
