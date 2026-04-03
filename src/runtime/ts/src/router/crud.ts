import { Orm, HttpResult } from "../ui/backend.js";
import { Model } from "../cidl.js";
import { ApiImplementation } from "./router.js";

export function crudRoute(
  meta: Model,
  methodName: string,
  env: any,
): ApiImplementation | null {
  switch (methodName) {
    case "Get":
      return (...args) => get(meta, args, env);
    case "List":
      return (...args) => list(meta, args, env);
    case "Upsert":
      return (body, ds) => upsert(meta, body, ds as string, env);
    default:
      return null;
  }
}

async function upsert(
  meta: Model,
  body: any,
  dataSourceRef: string,
  env: any,
): Promise<HttpResult<unknown>> {
  const dataSource = meta.data_sources[dataSourceRef];
  const orm = Orm.fromEnv(env);

  // Upsert
  let result: unknown | null = null;
  try {
    result = await orm.upsert(meta, body, dataSource.gen.tree);
  } catch {
    return HttpResult.fail(400);
  }

  return !result ? HttpResult.fail(404) : HttpResult.ok(200, result);
}

async function get(
  meta: Model,
  args: any[],
  env: any,
): Promise<HttpResult<unknown>> {
  // Last arg is always the data source
  const dataSourceRef = args.pop();

  const dataSource = meta.data_sources[dataSourceRef];
  const orm = Orm.fromEnv(env);

  // With N data source get params, and M key fields, there are N+M args passed.
  const N = dataSource.get!.parameters.length;
  const M = meta.key_fields.length;

  const dataSourceArgs = args.slice(0, N);
  const keyFieldArgs = args.slice(N, N + M).reduce(
    (acc, val, i) => {
      acc[meta.key_fields[i]] = val;
      return acc;
    },
    {} as Record<string, any>,
  );

  const res = await orm.getQuery(
    meta,
    dataSource.gen.get!(env, ...dataSourceArgs),
    dataSource.gen.tree,
    keyFieldArgs,
  );
  if (res === null) {
    return HttpResult.fail(404);
  }
  return HttpResult.ok(200, res);
}

async function list(
  meta: Model,
  args: any[],
  env: any,
): Promise<HttpResult<unknown>> {
  // Last arg is always the data source
  const dataSourceRef = args.pop();

  const dataSource = meta.data_sources[dataSourceRef];
  const orm = Orm.fromEnv(env);

  const dataSourceArgs = args.slice(0, dataSource.list!.parameters.length);
  const res = await orm.listQuery(
    meta,
    dataSource.gen.list!(env, ...dataSourceArgs),
    dataSource.gen.tree,
  );
  return HttpResult.ok(200, res);
}
