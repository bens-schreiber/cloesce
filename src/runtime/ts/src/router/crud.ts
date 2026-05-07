import { Orm, HttpResult } from "../ui/backend.js";
import { ApiMethod, Model } from "../cidl.js";
import { ApiImplementation } from "./router.js";
import { CloesceError, CloesceResult, InternalError } from "../common.js";

/**
 * Parses a CRUD method name into its (verb, dataSourceName) pair.
 * - `$verb` -> { verb, dataSourceName: "Default" }
 * - `$verb_DsName` -> { verb, dataSourceName: "DsName" }
 * - Anything else -> null
 */
function parseCrudName(
  name: string,
): { verb: "get" | "list" | "save"; dataSourceName: string } | null {
  if (!name.startsWith("$")) return null;
  const rest = name.slice(1);
  const underscoreIdx = rest.indexOf("_");
  const verb = (underscoreIdx === -1 ? rest : rest.slice(0, underscoreIdx)) as
    | "get"
    | "list"
    | "save";
  if (verb !== "get" && verb !== "list" && verb !== "save") return null;
  const dataSourceName = underscoreIdx === -1 ? "Default" : rest.slice(underscoreIdx + 1);
  return { verb, dataSourceName };
}

export function crudRoute(meta: Model, method: ApiMethod, env: any): ApiImplementation | null {
  const parsed = parseCrudName(method.name);
  if (!parsed) return null;

  const { verb, dataSourceName } = parsed;
  if (!meta.data_sources[dataSourceName]) return null;

  switch (verb) {
    case "get":
      return (...args) => get(meta, args, dataSourceName, env);
    case "list":
      return (...args) => list(meta, args, dataSourceName, env);
    case "save":
      return (body) => upsert(meta, body, dataSourceName, env);
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
  args: any[],
  dataSourceRef: string,
  env: any,
): Promise<HttpResult<unknown>> {
  const dataSource = meta.data_sources[dataSourceRef];

  // Method parameters for $get_<DS> are: [...DS.get.parameters, ...model.key_fields]
  // (in this exact order, see semantic/src/crud.rs)
  const numGetParams = dataSource.get?.parameters.length ?? 0;
  const dataSourceArgs = args.slice(0, numGetParams);
  const keyArgs = args.slice(numGetParams, numGetParams + meta.key_fields.length);

  const res = await dataSource.gen.get(env, ...dataSourceArgs, ...keyArgs);

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
  args: any[],
  dataSourceRef: string,
  env: any,
): Promise<HttpResult<unknown>> {
  const dataSource = meta.data_sources[dataSourceRef];

  // Method parameters for $list_<DS> are exactly DS.list.parameters
  const numListParams = dataSource.list?.parameters.length ?? 0;
  const dataSourceArgs = args.slice(0, numListParams);

  const res = await dataSource.gen.list!(env, ...dataSourceArgs);
  if (res.errors.length > 0) {
    return HttpResult.fail(400, CloesceError.displayErrors(res as CloesceResult<never>));
  }

  return HttpResult.ok(200, res.value);
}
