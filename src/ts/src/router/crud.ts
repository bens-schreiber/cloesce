import { D1Database } from "@cloudflare/workers-types/experimental/index.js";
import { IncludeTree, Orm } from "../ui/backend.js";
import { HttpResult } from "../ui/common.js";
import { NO_DATA_SOURCE } from "../ast.js";

/**
 * Wraps an object in a Proxy that will intercept non-overriden CRUD methods,
 * calling a default implementation.
 */
export function proxyCrud(obj: any, ctor: any, d1: D1Database) {
  return new Proxy(obj, {
    get(target, method) {
      // If the instance defines the method, always use it (override allowed)
      const value = Reflect.get(target, method);
      if (typeof value === "function") {
        return value.bind(target);
      }

      // Fallback to CRUD methods
      if (method === "save") {
        return (body: object, ds: string) => upsert(ctor, body, ds, d1);
      }

      if (method === "list") {
        return (ds: string) => list(ctor, ds, d1);
      }

      if (method === "get") {
        return (id: any, ds: string) => get(ctor, id, ds, d1);
      }

      return value;
    },
  });
}

async function upsert(
  ctor: any,
  body: object,
  dataSource: string,
  d1: D1Database,
): Promise<HttpResult<unknown>> {
  const includeTree = findIncludeTree(dataSource, ctor);
  const orm = Orm.fromD1(d1);

  const result = await orm.upsert(ctor, body, includeTree);
  if (result.isLeft()) return HttpResult.fail(500, result.value);

  const getRes = await orm.get(ctor, result.value, includeTree);
  return getRes.isRight()
    ? HttpResult.ok(200, getRes.value)
    : HttpResult.fail(500, getRes.value);
}

async function get(
  ctor: any,
  id: any,
  dataSource: string,
  d1: D1Database,
): Promise<HttpResult<unknown>> {
  const includeTree = findIncludeTree(dataSource, ctor);

  const orm = Orm.fromD1(d1);
  const res = await orm.get(ctor, id, includeTree);

  return res.isRight()
    ? HttpResult.ok(200, res.value)
    : HttpResult.fail(500, res.value);
}

async function list(
  ctor: any,
  dataSource: string,
  d1: D1Database,
): Promise<HttpResult<unknown>> {
  const includeTree = findIncludeTree(dataSource, ctor);

  const orm = Orm.fromD1(d1);
  const res = await orm.list(ctor, { includeTree });

  return res.isRight()
    ? HttpResult.ok(200, res.value)
    : HttpResult.fail(500, res.value as string);
}

function findIncludeTree(
  dataSource: string,
  ctor: new () => object,
): IncludeTree<any> | null {
  const normalizedDs = dataSource === NO_DATA_SOURCE ? null : dataSource;
  return normalizedDs ? (ctor as any)[normalizedDs] : null;
}
