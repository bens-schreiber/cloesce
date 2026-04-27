import { Orm, HttpResult } from "../ui/backend.js";
import { ApiMethod, Model } from "../cidl.js";
import { ApiImplementation } from "./router.js";
import { CloesceError, CloesceResult, InternalError } from "../common.js";

export function crudRoute(meta: Model, method: ApiMethod, env: any): ApiImplementation | null {
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

  let result: CloesceResult<unknown | null>;
  try {
    result = await orm.upsert(meta, body, dataSource.gen.include);
  } catch (e) {
    throw new InternalError(`Upsert failed: ${JSON.stringify(e)}`);
  }

  if (result.errors.length > 0) {
    return HttpResult.fail(400, CloesceError.displayErrors(result as CloesceResult<never>));
  }

  if (result.value === null) {
    return HttpResult.fail(404);
  }

  return HttpResult.ok(200, result.value);
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

  // Build a lookup from method parameter name to its value
  const paramValues = new Map<string, unknown>();
  for (let i = 0; i < method.parameters.length; i++) {
    paramValues.set(method.parameters[i].name, args[i]);
  }

  // Data source params are prefixed with the source name in the method signature.
  // All DS params are nullable in the method signature to support multi-source schemas,
  // but once the source is selected they are required.
  const dataSourceArgs: unknown[] = [];
  for (const p of dataSource.get?.parameters ?? []) {
    const val = paramValues.get(`${dataSourceRef}_${p.name}`);
    if (val === null || val === undefined) return HttpResult.fail(400);
    dataSourceArgs.push(val);
  }

  // Key fields are unprefixed
  const keyFieldArgs = Object.fromEntries(
    meta.key_fields.map((f) => [f.name, paramValues.get(f.name)]),
  );

  const res = await dataSource.gen.get(env, ...dataSourceArgs, ...Object.values(keyFieldArgs));

  if (res.errors.length > 0) {
    return HttpResult.fail(400, CloesceError.displayErrors(res as CloesceResult<never>));
  }

  if (res.value === null) {
    return HttpResult.fail(404);
  }

  return HttpResult.ok(200, res.value);
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

  // Build a lookup from method parameter name to its value
  const paramValues = new Map<string, unknown>();
  for (let i = 0; i < method.parameters.length; i++) {
    paramValues.set(method.parameters[i].name, args[i]);
  }

  // Data source params are prefixed with the source name in the method signature.
  // All DS params are nullable in the method signature to support multi-source schemas,
  // but once the source is selected they are required.
  const dataSourceArgs: unknown[] = [];
  for (const p of dataSource.list!.parameters) {
    const val = paramValues.get(`${dataSourceRef}_${p.name}`);
    if (val === null || val === undefined) return HttpResult.fail(400);
    dataSourceArgs.push(val);
  }

  const res = await dataSource.gen.list!(env, ...dataSourceArgs);
  if (res.errors.length > 0) {
    return HttpResult.fail(400, CloesceError.displayErrors(res as CloesceResult<never>));
  }

  return HttpResult.ok(200, res.value);
}
