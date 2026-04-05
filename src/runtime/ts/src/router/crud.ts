import { Orm, HttpResult } from "../ui/backend.js";
import { ApiMethod, Cidl, DataSource, Field, Model } from "../cidl.js";
import { ApiImplementation, MatchedRoute } from "./router.js";

export function crudRoute(
  meta: Model,
  method: ApiMethod,
  env: any,
): ApiImplementation | null {
  switch (method.name) {
    case "$get":
      return (...args) => get(meta, method, args, env);
    case "$list":
      return (...args) => list(meta, method, args, env);
    case "$save":
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
  method: ApiMethod,
  args: any[],
  env: any,
): Promise<HttpResult<unknown>> {
  // Last arg is always the data source
  const dataSourceRef = args.pop();

  const dataSource = meta.data_sources[dataSourceRef];

  // Args is the union of all data source get params across all data sources for the model,
  // plus the key fields for the model. To find the parameters for this specific data source, 
  // we have to take the intersection of the data source get params and the args passed in.
  const paramOrder = new Map(
    dataSource.get!.parameters.map((p, i) => [p.name, i])
  );
  const keyFields = new Set(meta.key_fields);
  const dataSourceArgs: unknown[] = new Array(paramOrder.size);
  const keyFieldArgs: Record<string, any> = {};

  for (let i = 0; i < method.parameters.length; i++) {
    const paramName = method.parameters[i].name;
    const arg = args[i];

    const dsIndex = paramOrder.get(paramName);
    if (dsIndex !== undefined) {
      dataSourceArgs[dsIndex] = arg;
    }

    if (keyFields.has(paramName)) {
      keyFieldArgs[paramName] = arg;
    }
  }

  const res = await dataSource.gen.get(env, ...dataSourceArgs, ...Object.values(keyFieldArgs));
  if (res === null) {
    return HttpResult.fail(404);
  }
  return HttpResult.ok(200, res);
}

async function list(
  meta: Model,
  method: ApiMethod,
  args: any[],
  env: any,
): Promise<HttpResult<unknown>> {
  // Last arg is always the data source
  const dataSourceRef = args.pop();

  const dataSource = meta.data_sources[dataSourceRef];

  // Args is the union of all data source get params across all data sources for the model.
  // To find the parameters for this specific data source, we have to take the intersection
  // of the data source get params and the args passed in, while preserving the order.
  const paramOrder = new Map(
    dataSource.list!.parameters.map((p, i) => [p.name, i])
  );
  const dataSourceArgs: unknown[] = new Array(paramOrder.size);

  for (let i = 0; i < method.parameters.length; i++) {
    const paramName = method.parameters[i].name;
    const index = paramOrder.get(paramName);

    if (index !== undefined) {
      dataSourceArgs[index] = args[i];
    }
  }

  const res = await dataSource.gen.list!(env, ...dataSourceArgs);
  return HttpResult.ok(200, res);
}