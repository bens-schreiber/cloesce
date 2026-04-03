import { Orm, HttpResult } from "../ui/backend.js";
import { Model } from "../cidl.js";
import { ApiImplementation } from "./router.js";
// import { DataSource } from "./orm.js";
// import { findDataSource, RuntimeContainer } from "./router.js";

export function crudRoute(
  meta: Model,
  methodName: string,
  env: any,
): ApiImplementation | null {
  switch (methodName) {
    case "Get":
      return (...args) => get(meta, args, env);
    case "List":
      return list;
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
  const db = env[meta.d1_binding!];

  // Upsert
  let result: unknown | null = null;
  try {
    result = await orm.upsert(meta, db, body, dataSource.include);
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
  const dataSourceRef = args.pop();
  const dataSource = meta.data_sources[dataSourceRef];
  const orm = Orm.fromEnv(env);

  let result = null
  if (!dataSource.get) {
    const keyFields = Object.fromEntries(meta.key_fields.map((k) => [k, args.pop()]));
    result = await orm.getCustom(meta, null, {}, keyFields, dataSource.include)
  } else {
    const stmt = dataSource.get(env, ...args);
    result = await orm.getQuery(meta, stmt, dataSource.include, {});
  }


  // const getArgs: {
  //   primaryKey?: any;
  //   keyParams?: Record<string, any>;
  //   include?: DataSource<any>;
  // } = {};

  // let argIndex = 0;
  // if (model.primary_key_columns.length > 0) {
  //   // Primary key arguments are ordered by the compiler.
  //   getArgs.primaryKey = {};
  //   for (const pkCol of model.primary_key_columns) {
  //     getArgs.primaryKey[pkCol.value.name] = args[argIndex++];
  //   }
  // }

  // if (model.key_params.length > 0) {
  //   // All key params come after the primary key.
  //   // Order is guaranteed by the compiler.
  //   getArgs.keyParams = {};
  //   for (const keyParam of model.key_params) {
  //     getArgs.keyParams[keyParam] = args[argIndex++];
  //   }
  // }

  // // The last argument is always the data source.
  // getArgs.include = findDataSource(ctor, args[argIndex]);

  // const orm = Orm.fromEnv(env);
  // const result: any | null = await orm.get(ctor, getArgs);
  // return !result ? HttpResult.fail(404) : HttpResult.ok(200, result);
}

async function list(
  ctor: any,
  args: any[],
  env: any,
): Promise<HttpResult<unknown>> {
  const { ast } = RuntimeContainer.get();
  const model = ast.models[ctor.name];

  let argIndex = 0;
  const lastSeenValues = model.primary_key_columns.map(() => args[argIndex++]);
  const limit = args[argIndex++];
  const offset = args[argIndex++];
  const dataSourceRef = args[argIndex];

  // Last seen can only be used if all primary key values are present.
  // Fail gracefully by ignoring lastSeen if some values are missing.
  const lastSeen =
    lastSeenValues.length == model.primary_key_columns.length &&
      !lastSeenValues.some((v) => v == null)
      ? Object.fromEntries(
        model.primary_key_columns.map((col, i) => [
          col.value.name,
          lastSeenValues[i],
        ]),
      )
      : undefined;

  const dataSource = findDataSource(ctor, dataSourceRef);
  const orm = Orm.fromEnv(env);

  const result: any[] = await orm.list(ctor, {
    include: dataSource,
    lastSeen,
    limit,
    offset,
  });
  return HttpResult.ok(200, result);
}
