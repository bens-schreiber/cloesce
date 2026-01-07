import { IncludeTree, Orm } from "../ui/backend.js";
import { HttpResult } from "../ui/common.js";
import { NO_DATA_SOURCE } from "../ast.js";

/**
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
      if (method === "save") {
        return (body: object, ds: string) => upsert(ctor, body, ds, env);
      }

      if (method === "list") {
        return (ds: string) => list(ctor, ds, env);
      }

      if (method === "get") {
        return (id: any, ds: string) => _get(ctor, id, ds, env);
      }

      return value;
    },
  });
}

async function upsert(
  ctor: any,
  body: object,
  dataSource: string,
  env: any,
): Promise<HttpResult<unknown>> {
  const includeTree = findIncludeTree(dataSource, ctor);
  const orm = Orm.fromEnv(env);

  // Upsert
  const result = await orm.upsert(ctor, body, includeTree);
  if (result.isLeft()) {
    return HttpResult.fail(500, result.value.error);
  }

  // Get
  const getResult = await orm.get(ctor, result.unwrap(), includeTree);
  if (getResult.isLeft()) {
    return HttpResult.fail(500, result.value.error);
  }

  const model: any | null = getResult.unwrap();
  return !model ? HttpResult.fail(404) : HttpResult.ok(200, model);
}

async function _get(
  ctor: any,
  id: any,
  dataSource: string,
  env: any,
): Promise<HttpResult<unknown>> {
  const includeTree = findIncludeTree(dataSource, ctor);

  const orm = Orm.fromEnv(env);
  const result = await orm.get(ctor, id, includeTree);

  if (result.isLeft()) {
    return HttpResult.fail(500, result.value.error);
  }

  const model: any | null = result.unwrap();
  return !model ? HttpResult.fail(404) : HttpResult.ok(200, model);
}

async function list(
  ctor: any,
  dataSource: string,
  env: any,
): Promise<HttpResult<unknown>> {
  const includeTree = findIncludeTree(dataSource, ctor);

  const orm = Orm.fromEnv(env);
  const result = await orm.list(ctor, { includeTree });

  if (result.isLeft()) {
    return HttpResult.fail(500, result.value.error);
  }

  const models: any[] = result.unwrap();
  return HttpResult.ok(200, models);
}

function findIncludeTree(
  dataSource: string,
  ctor: new () => object,
): IncludeTree<any> | null {
  const normalizedDs = dataSource === NO_DATA_SOURCE ? null : dataSource;
  return normalizedDs ? (ctor as any)[normalizedDs] : null;
}
