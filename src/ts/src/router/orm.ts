import type {
  R2Bucket,
  R2ObjectBody,
  D1Database,
  KVNamespace,
  D1Result,
} from "@cloudflare/workers-types";

import { ConstructorRegistry, RuntimeContainer } from "../router/router.js";
import { WasmResource, invokeOrmWasm } from "../router/wasm.js";
import {
  Model as AstModel,
  CidlType,
  CloesceAst,
  CrudListParam,
  D1Column,
  getNavigationPropertyCidlType,
} from "../ast.js";
import { InternalError, u8ToB64 } from "../common.js";
import { IncludeTree, DeepPartial, KValue, Paginated } from "../ui/backend.js";

/**
 * Defines a Data Source for a Model, which can include
 * KV, R2, 1:1, 1:M and M:M relationships as specified by the include tree.
 */
export interface DataSource<T> {
  /**
   * The include tree specifying which relationships to include in the data source.
   */
  includeTree?: IncludeTree<T>;

  /**
   * A custom function called when using `orm.get`. Defaults to:
   *
   * ```ts
   * `${Orm.select(ctor, { include: includeTree })} WHERE ${pkName1} = ? AND ${pkName2} = ? ...`
   * ```
   *
   * Parameters for each primary key column are always bound to the query when executed by D1,
   * in primary key column order. Reference them in the query using `?`, `?1`, `?2`, etc.
   *
   * @param joined A helper function to generate a SELECT query for the model with the same include tree as the data source.
   * @return A SQL query string to retrieve a single instance of the model from D1.
   */
  get?: (joined: (from?: string) => string) => string;

  /**
   * A custom function called when using `orm.list`. Defaults to a seek pagination query:
   * ```ts
   * `${Orm.select(ctor, { include: includeTree })} WHERE ("${model.name}"."${pk1}", ...) > (?, ...) ORDER BY "${model.name}"."${pk1}" ASC, ... LIMIT ?`
   * ```
   *
   * Use `DataSource.listParams` to specify which parameters to bind when calling `orm.list`.
   * If a custom implementation is given, no parameters are bound by default,
   * and it's the responsibility of the user to specify and bind any parameters needed for the query.
   *
   *
   * @param joined A helper function to generate a SELECT query for the model with the same include tree as the data source.
   * @returns A SQL query string to retrieve multiple instances of the model from D1.
   */
  list?: (joined: (from?: string) => string) => string;

  /**
   * The parameters to bind when calling `DataSource.list`. Defaults to empty.
   */
  listParams?: CrudListParam[];
}

type Include<T> = DataSource<T> | IncludeTree<T>;
function isDataSource<T>(include: Include<T>): include is DataSource<T> {
  return (
    "includeTree" in include ||
    "list" in include ||
    "get" in include ||
    "listParams" in include
  );
}
function getTreeFromInclude<T>(
  include: Include<T> | null | undefined,
): IncludeTree<T> {
  if (!include) {
    return {} as IncludeTree<T>;
  }
  return isDataSource(include)
    ? ((include.includeTree as IncludeTree<T>) ?? ({} as IncludeTree<T>))
    : include;
}

export class Orm {
  private constructor(private env: unknown) {}

  /**
   * Creates an instance of an `Orm`
   * @param env The Wrangler environment containing Cloudflare bindings.
   */
  static fromEnv(env: unknown): Orm {
    // TODO: We could validate that `env` is of the correct type defined by the `＠WranglerEnv` class
    // by putting the class definition in the Constructor Registry at compile time.
    return new Orm(env);
  }

  // TODO: support multiple D1 bindings
  private get db(): D1Database {
    const { ast } = RuntimeContainer.get();
    return (this.env as any)[ast.wrangler_env!.d1_binding!];
  }

  /**
   * Given a model, retrieves the default data source for the model, which includes
   * all KV, R2, 1:1, 1:M and M:M relationships.
   *
   * Does not include nested relationships of 1:M and M:M relationships to avoid excessively large data retrievals.
   *
   * @param ctor Constructor of the model to retrieve the default data source for
   * @returns The default data source for the model.
   */
  static defaultDataSource<T>(ctor: new () => T): DataSource<T> {
    const defaultDs: DataSource<T> | undefined = (ctor as any)["default"];
    if (defaultDs && defaultDs.includeTree) {
      return (ctor as any)["default"] as DataSource<T>;
    }

    const { ast } = RuntimeContainer.get();
    const dataSourceMeta = ast.models[ctor.name]?.data_sources["default"];
    if (!dataSourceMeta) {
      throw new InternalError(
        `No default data source found for model ${ctor.name}`,
      );
    }

    return {
      includeTree: dataSourceMeta.tree as IncludeTree<T>,
    };
  }

  /**
   * Maps D1 results into model instances. Capable of mapping a flat result set
   * (ie, SELECT * FROM Model) or a joined result granted it is aliased as `select_model` would produce.
   *
   * Does not hydrate into an instance of the model; for that, use `hydrate` after mapping.
   *
   * @example
   * ```ts
   * const d1Result = await db.prepare("SELECT * FROM User").all();
   * const users: User[] = Orm.map(User, d1Result.results);
   * ```
   *
   * @example
   * ```ts
   * const d1Result = await db.prepare(`
   *  ${Orm.select(User, null, { posts: {} })}
   *  WHERE User.id = ?
   * `).bind(1).all();
   *
   * const users: User[] = Orm.map(User, d1Result.results, { posts: {} });
   * ```
   *
   * @param ctor Constructor of the model to map to
   * @param d1Results Results from a D1 query
   * @param include Include Tree or DataSource specifying which navigation properties to include in the mapping.
   * @returns Array of mapped model instances
   */
  static map<T extends object>(
    ctor: new () => T,
    d1Results: D1Result,
    include: Include<T> = {},
  ): T[] {
    const { wasm } = RuntimeContainer.get();
    const d1ResultsRes = WasmResource.fromString(
      JSON.stringify(d1Results.results),
      wasm,
    );

    const tree = getTreeFromInclude(include);

    const includeTreeRes = WasmResource.fromString(JSON.stringify(tree), wasm);
    const mapQueryRes = invokeOrmWasm(
      wasm.map,
      [WasmResource.fromString(ctor.name, wasm), d1ResultsRes, includeTreeRes],
      wasm,
    );

    if (mapQueryRes.isLeft()) {
      throw new InternalError(`Mapping failed: ${mapQueryRes.value}`);
    }

    return JSON.parse(mapQueryRes.unwrap()) as T[];
  }

  /**
   * Generates a SELECT query string for a given Model,
   * retrieving the model and its relations aliased as JSON.
   *
   * @param ctor - Constructor of the model to select
   * @param args - Arguments specifying which relations/fields to select
   * @returns The generated SELECT query string
   *
   * @example
   * ```ts
   * Orm.select(Boss, Boss.withAll);
   *
   * // Example result:
   * const result = `
   * SELECT
   *   "Boss"."id" AS "id",
   *   "Person_1"."id" AS "persons.id",
   *   "Person_1"."bossId" AS "persons.bossId",
   *   "Dog_2"."id" AS "persons.dogs.id",
   *   "Dog_2"."personId" AS "persons.dogs.personId",
   *   "Cat_3"."id" AS "persons.cats.id",
   *   "Cat_3"."personId" AS "persons.cats.personId"
   * FROM "Boss"
   * LEFT JOIN "Person" AS "Person_1"
   *   ON "Boss"."id" = "Person_1"."bossId"
   * LEFT JOIN "Dog" AS "Dog_2"
   *   ON "Person_1"."id" = "Dog_2"."personId"
   * LEFT JOIN "Cat" AS "Cat_3"
   *   ON "Person_1"."id" = "Cat_3"."personId"
   * `;
   * ```
   */
  static select<T extends object>(
    ctor: new () => T,
    args: {
      from?: string | null;
      include?: Include<T>;
    } = {
      from: null,
      include: {},
    },
  ): string {
    const { wasm } = RuntimeContainer.get();
    const fromRes = WasmResource.fromString(
      JSON.stringify(args.from ?? null),
      wasm,
    );

    const include = args.include ?? {};
    const tree = getTreeFromInclude(include);
    const includeTreeRes = WasmResource.fromString(JSON.stringify(tree), wasm);

    const selectQueryRes = invokeOrmWasm(
      wasm.select_model,
      [WasmResource.fromString(ctor.name, wasm), fromRes, includeTreeRes],
      wasm,
    );

    if (selectQueryRes.isLeft()) {
      throw new InternalError(
        `Select generation failed: ${selectQueryRes.value}`,
      );
    }

    return selectQueryRes.unwrap();
  }

  /**
   * Given a base object representing a Model, hydrates its D1, R2 and KV properties.
   * Fetches all KV and R2 data concurrently.
   * @param ctor Constructor of the model to hydrate
   * @param args Arguments for hydration
   * @returns The hydrated model instance
   */
  async hydrate<T extends object>(
    ctor: new () => T,
    args: {
      base?: any;
      keyParams?: Record<string, string>;
      include?: Include<T>;
    } = {
      base: {},
      keyParams: {},
      include: {},
    },
  ): Promise<T> {
    const { ast, constructorRegistry } = RuntimeContainer.get();
    if (!ast.models[ctor.name]) {
      return args.base ?? ({} as T);
    }

    const modelCidlType: CidlType = {
      Object: ctor.name,
    };

    const env: any = this.env;
    const promises: Promise<void>[] = [];
    const tree = getTreeFromInclude(args.include ?? {});

    const hydrated = hydrateType(args.base ?? {}, modelCidlType, {
      ast,
      ctorReg: constructorRegistry,
      includeTree: tree,
      keyParams: args.keyParams ?? {},
      env,
      promises,
    });

    await Promise.all(promises);
    return hydrated;
  }

  /**
   * Given a new Model object, performs an upsert operation for D1 and KV.
   *
   * Concurrently performs all D1 and KV operations.
   *
   * Some KV results depend on a successful D1 upsert to resolve their keys,
   * and will be uploaded only after the D1 upsert completes.
   *
   * If a Model is missing a primary key, and that primary key is of Integer type,
   * it will be auto-incremented by D1. Else, upsert will fail if the primary key is missing.
   *
   * @param ctor Constructor of the model to upsert
   * @param newModel The new model object to upsert
   * @param include Include tree specifying which navigation properties to include
   * @returns The upserted model instance, or `null` if upsert failed
   */
  // TODO: Better ORM error handling strategies
  async upsert<T extends object>(
    ctor: new () => T,
    newModel: DeepPartial<T>,
    include: Include<T> = {},
  ): Promise<T | null> {
    const { wasm, ast } = RuntimeContainer.get();
    const meta = ast.models[ctor.name];

    const includeTree = getTreeFromInclude(include);
    const upsertQueryRes = invokeOrmWasm(
      wasm.upsert_model,
      [
        WasmResource.fromString(ctor.name, wasm),
        WasmResource.fromString(
          // TODO: Stringify only objects in the include tree?
          // Could try to track `this` in the reviver function
          JSON.stringify(newModel, (_, v) =>
            // To serialize a Uint8Array s.t. WASM can read it, we convert it to a base64 string.
            v instanceof Uint8Array ? u8ToB64(v) : v,
          ),
          wasm,
        ),
        WasmResource.fromString(JSON.stringify(includeTree), wasm),
      ],
      wasm,
    );
    if (upsertQueryRes.isLeft()) {
      throw new Error(`Upsert failed: ${upsertQueryRes.value}`);
    }

    const res = JSON.parse(upsertQueryRes.unwrap()) as {
      sql: {
        query: string;
        values: any[];
      }[];
      kv_uploads: {
        namespace_binding: string;
        key: string;
        value: any;
        metadata: unknown;
      }[];
      kv_delayed_uploads: {
        path: string[];
        namespace_binding: string;
        key: string;
        value: any;
        metadata: unknown;
      }[];
    };

    const kvUploadPromises: Promise<void>[] = res.kv_uploads.map(
      async (upload) => {
        const namespace: KVNamespace | undefined = (this.env as any)[
          upload.namespace_binding
        ];
        if (!namespace) {
          throw new InternalError(
            `KV Namespace binding "${upload.namespace_binding}" not found for upsert.`,
          );
        }

        await namespace.put(upload.key, JSON.stringify(upload.value), {
          metadata: upload.metadata,
        });
      },
    );

    const queries = res.sql.map((s) =>
      this.db.prepare(s.query).bind(...s.values),
    );

    // Concurrently execute SQL with KV uploads.
    const [batchRes] = await Promise.all([
      queries.length > 0 ? this.db.batch(queries) : Promise.resolve([]),
      ...kvUploadPromises,
    ]);

    let base = {};
    if (queries.length > 0) {
      const failed = batchRes.find((r) => !r.success);
      if (failed) {
        // An error in the upsert should not be possible unless the AST is invalid.
        throw new InternalError(
          `Upsert failed during execution: ${failed.error}`,
        );
      }

      // A SELECT statement towards the end will call `select_model` to retrieve the upserted model.
      let selectIndex: number;
      for (let i = res.sql.length - 1; i >= 0; i--) {
        if (/^SELECT/i.test(res.sql[i].query)) {
          selectIndex = i;
          break;
        }
      }

      base = Orm.map(ctor, batchRes[selectIndex!], includeTree)[0];
    }

    // Base needs to include all of the key params from newModel and its includes.
    const q: Array<{ model: any; meta: AstModel; tree: IncludeTree<any> }> = [
      { model: base, meta, tree: includeTree },
    ]!;
    while (q.length > 0) {
      const {
        model: currentModel,
        meta: currentMeta,
        tree: currentTree,
      } = q.shift()!;

      // Key params
      for (const keyParam of currentMeta.key_params) {
        if (currentModel[keyParam] === undefined) {
          currentModel[keyParam] = (newModel as any)[keyParam];
        }
      }

      // Navigation properties
      for (const navProp of currentMeta.navigation_properties) {
        const nestedTree = currentTree[navProp.var_name];
        if (!nestedTree) continue;

        const nestedMeta = ast.models[navProp.model_reference];
        const value = currentModel[navProp.var_name];

        if (Array.isArray(value)) {
          for (let i = 0; i < value.length; i++) {
            q.push({
              model: value[i],
              meta: nestedMeta,
              tree: nestedTree,
            });
          }
        } else if (value) {
          q.push({
            model: value,
            meta: nestedMeta,
            tree: nestedTree,
          });
        }
      }
    }

    // Upload all delayed KV uploads
    await Promise.all(
      res.kv_delayed_uploads.map(async (upload) => {
        let current: any = base;
        for (const pathPart of upload.path) {
          current = current[pathPart];
          if (current === undefined) {
            throw new InternalError(
              `Failed to resolve path ${upload.path.join(".")} for delayed KV upload.`,
            );
          }
        }

        const namespace: KVNamespace | undefined = (this.env as any)[
          upload.namespace_binding
        ];
        if (!namespace) {
          throw new InternalError(
            `KV Namespace binding "${upload.namespace_binding}" not found for upsert.`,
          );
        }

        const key = resolveKey(upload.key, current, {});
        if (!key) {
          throw new InternalError(
            `Failed to resolve key format "${upload.key}" for delayed KV upload.`,
          );
        }

        await namespace.put(key, JSON.stringify(upload.value), {
          metadata: upload.metadata,
        });
      }),
    );

    // Hydrate and return the upserted model
    return await this.hydrate(ctor, {
      base: base,
      include: includeTree,
    });
  }

  /**
   * Lists all instances of a given Model from D1.
   * A model without a primary key cannot be listed, and this method will return an empty array in that case.
   *
   * @param ctor Constructor of the model to list
   * @param args Arguments for listing, such as the include tree and pagination parameters
   * @returns Array of listed model instances
   */
  async list<T extends object>(
    ctor: new () => T,
    args?: {
      include?: Include<T>;
      lastSeen?: Partial<T>;
      limit?: number;
      offset?: number;
    },
  ): Promise<T[]> {
    const { ast } = RuntimeContainer.get();
    const model = ast.models[ctor.name];
    if (!model) {
      return [];
    }

    if (model.primary_key_columns.length < 1) {
      // Listing is not supported for models without primary keys (i.e., KV or R2 only).
      return [];
    }

    args ??= {};
    args.include ??= {};
    args.lastSeen ??= defaultPrimaryKey<T>(model.primary_key_columns);
    args.limit ??= 1000;

    const lastSeenValues = getPrimaryKeyValues(
      ctor.name,
      model.primary_key_columns,
      args.lastSeen,
      "lastSeen",
    );

    let usedDefaultQuery = false;
    let query: string;
    if (isDataSource(args.include) && args.include.list) {
      // Override the default list generation with a custom one provided by the user.
      const includeDs = args.include as DataSource<T>;
      query = args.include.list((from) =>
        Orm.select(ctor, { from, include: includeDs.includeTree }),
      );
    } else {
      // Default list query with seek pagination
      const tupleCols = model.primary_key_columns
        .map((col) => `"${model.name}"."${col.value.name}"`)
        .join(", ");
      const tupleParams = model.primary_key_columns.map(() => "?").join(", ");
      const orderBy = model.primary_key_columns
        .map((col) => `"${model.name}"."${col.value.name}" ASC`)
        .join(", ");
      query = `
        ${Orm.select(ctor, { include: args.include })}
        WHERE (${tupleCols}) > (${tupleParams})
        ORDER BY ${orderBy}
        LIMIT ?
      `;
      usedDefaultQuery = true;
    }

    let listParams: CrudListParam[];
    if (isDataSource(args.include) && args.include.list) {
      listParams = args.include.listParams ?? [];
    } else {
      listParams = ["LastSeen", "Limit"];
    }

    const bindValues: any[] = [];
    for (const param of listParams) {
      switch (param) {
        case "LastSeen":
          bindValues.push(...lastSeenValues);
          break;
        case "Limit":
          bindValues.push(args.limit);
          break;
        case "Offset":
          bindValues.push(args.offset);
          break;
      }
    }

    const rows = await this.db
      .prepare(query)
      .bind(...bindValues)
      .all();
    if (rows.error) {
      if (usedDefaultQuery) {
        // An error in the default query should not be possible unless the AST is invalid.
        throw new InternalError(
          `Failed to list models for ${ctor.name} with default query: ${rows.error}`,
        );
      }

      // TODO: We should have a better error handling strategy than just throwing generic errors, since
      // an error in the query is entirely possible from invalid custom list functions.
      throw new Error(`Failed to list models for ${ctor.name}: ${rows.error}`);
    }

    // Map and hydrate
    const results = Orm.map(ctor, rows, args.include ?? {});
    await Promise.all(
      results.map(async (modelJson, index) => {
        results[index] = await this.hydrate(ctor, {
          base: modelJson,
          include: args.include ?? {},
        });
      }),
    );

    return results;
  }

  /**
   * Fetches a model by its primary key ID or key parameters.
   * - If the model does not have a primary key, key parameters must be provided.
   * - If the model has primary key columns, `primaryKey` must provide each key column.
   *
   * @param ctor Constructor of the model to retrieve
   * @param args Arguments for retrieval
   * @returns The retrieved model instance, or `null` if not found
   */
  async get<T extends object>(
    ctor: new () => T,
    args?: {
      primaryKey?: Partial<T>;
      keyParams?: Record<string, string>;
      include?: Include<T>;
    },
  ): Promise<T | null> {
    const { ast } = RuntimeContainer.get();
    const model = ast.models[ctor.name];
    if (!model) {
      return null;
    }

    args ??= {};
    args.include ??= {};
    args.keyParams ??= {};

    // KV or R2 only
    if (model.primary_key_columns.length < 1) {
      return await this.hydrate(ctor, {
        keyParams: args.keyParams,
        include: args.include,
      });
    }

    const primaryKeyValues = getPrimaryKeyValues(
      ctor.name,
      model.primary_key_columns,
      args.primaryKey,
      "primaryKey",
    );

    let usedDefaultQuery = false;
    let query: string;
    if (isDataSource(args.include) && args.include.get) {
      // Override the default get generation with a custom one provided by the user.
      const includeDs = args.include as DataSource<T>;
      query = args.include.get((from) =>
        Orm.select(ctor, { from, include: includeDs.includeTree }),
      );
    } else {
      // Default get query
      const whereClause = model.primary_key_columns
        .map((col) => `"${model.name}"."${col.value.name}" = ?`)
        .join(" AND ");
      query = `
        ${Orm.select(ctor, { include: args.include })}
        WHERE ${whereClause}
      `;
      usedDefaultQuery = true;
    }

    const rows = await this.db
      .prepare(query)
      .bind(...primaryKeyValues)
      .all();
    if (rows.error) {
      if (usedDefaultQuery) {
        // An error in the default query should not be possible unless the AST is invalid.
        throw new InternalError(
          `Failed to retrieve model ${ctor.name} with default query: ${rows.error}`,
        );
      }

      // TODO: Better error handling strategy for errors from custom get functions.
      throw new Error(`Failed to retrieve model ${ctor.name}: ${rows.error}`);
    }

    if (rows.results.length < 1) {
      return null;
    }

    // Map and hydrate
    const results = Orm.map(ctor, rows, args.include ?? {});
    return await this.hydrate(ctor, {
      base: results[0],
      keyParams: args.keyParams,
      include: args.include ?? {},
    });
  }
}

/**
 * @returns An array of primary key values in the order of the model's primary key columns, extracted from `keyPartial`.
 */
function getPrimaryKeyValues(
  modelName: string,
  primaryKeyColumns: D1Column[],
  keyPartial: unknown,
  argName: "primaryKey" | "lastSeen",
): unknown[] {
  if (!keyPartial || typeof keyPartial !== "object") {
    throw new Error(
      `Failed to process ${argName} for model ${modelName}: expected an object containing all primary key columns`,
    );
  }

  const keys = keyPartial as Record<string, unknown>;
  const missing = primaryKeyColumns
    .filter(
      (col) =>
        keys[col.value.name] === undefined || keys[col.value.name] === null,
    )
    .map((col) => col.value.name);

  if (missing.length > 0) {
    throw new Error(
      `Failed to process ${argName} for model ${modelName}: missing primary key columns [${missing.join(", ")}]`,
    );
  }

  return primaryKeyColumns.map((col) => keys[col.value.name]);
}

/**
 * @returns An object containing default values for each primary key column.
 */
function defaultPrimaryKey<T extends object>(
  primaryKeyColumns: AstModel["primary_key_columns"],
): Partial<T> {
  const defaults: Partial<T> = {};
  for (const col of primaryKeyColumns) {
    (defaults as Record<string, unknown>)[col.value.name] = defaultLastSeen(
      col.value.cidl_type,
    );
  }
  return defaults;

  function defaultLastSeen(ty: CidlType): unknown {
    if (ty === "DateIso") {
      return new Date(0).toISOString();
    }

    if (ty === "Text") {
      return "";
    }

    return 0;
  }
}

/**
 * @returns null if any parameter could not be resolved
 */
function resolveKey(
  format: string,
  current: any,
  keyParams: Record<string, string>,
): string | null {
  try {
    return format.replace(/\{([^}]+)\}/g, (_, paramName) => {
      const paramValue = keyParams[paramName] ?? current[paramName];
      if (!paramValue) throw null;
      return String(paramValue);
    });
  } catch {
    return null;
  }
}

/**
 * @internal
 *
 * Hydrates a pure JSON value to a JS object based on it's CIDL type.
 *
 * @param value The value to hydrate.
 * @param cidlType The CIDL type of the value.
 * @param includeTree The include tree specifying which navigation properties to include.
 *  If null, includes all navigation properties, but does not hydrate KV and R2 properties.
 *
 * @returns The hydrated value if a transformation was necessary, or undefined if the value was mutated in place.
 */
export function hydrateType(
  value: any,
  cidlType: CidlType,
  args: {
    ast: CloesceAst;
    ctorReg: ConstructorRegistry;
    includeTree: IncludeTree<any> | null;
    keyParams: Record<string, string>;
    env: any;
    promises: Promise<void>[];
  },
): any | undefined {
  if (value === null || value === undefined) {
    return value;
  }

  // Unwrap nullable types
  if (typeof cidlType === "object" && "Nullable" in cidlType) {
    cidlType = cidlType.Nullable;
  }

  if (typeof cidlType !== "object") {
    switch (cidlType) {
      case "DateIso": {
        return new Date(value);
      }
      case "Blob": {
        const arr: number[] = value;
        return new Uint8Array(arr);
      }
      case "Boolean": {
        return Boolean(value);
      }
      default: {
        return value;
      }
    }
  }

  if ("Array" in cidlType) {
    if (!Array.isArray(value)) {
      return [];
    }

    for (let i = 0; i < value.length; i++) {
      value[i] = hydrateType(value[i], cidlType.Array, args);
    }

    // Arrays are hydrated in place since they are mutable,
    // so we return undefined to signal that no replacement is necessary.
    return undefined;
  }

  const objectName =
    "Object" in cidlType
      ? cidlType.Object
      : "Partial" in cidlType
        ? cidlType.Partial
        : null;
  if (objectName === null) {
    // Unsupported or unnecessary hydration.
    return value;
  }

  const modelMeta = args.ast.models[objectName];
  const pooMeta = args.ast.poos[objectName];
  const ctor = args.ctorReg[objectName];

  const instance = Object.assign(new ctor(), value);
  if (modelMeta) {
    // Hydrate columns
    for (const col of modelMeta.columns) {
      if (instance[col.value.name] === undefined) {
        continue;
      }

      const res = hydrateType(
        instance[col.value.name],
        col.value.cidl_type,
        args,
      );
      if (res) {
        instance[col.value.name] = res;
      }
    }

    // Recursively hydrate navigation properties
    for (const navProp of modelMeta.navigation_properties) {
      const tree = args.includeTree ? args.includeTree[navProp.var_name] : null;
      if (tree === undefined) continue;

      const res = hydrateType(
        instance[navProp.var_name],
        getNavigationPropertyCidlType(navProp),
        {
          ...args,
          includeTree: tree,
        },
      );
      if (res) {
        instance[navProp.var_name] = res;
      }
    }

    // Hydrate key params
    for (const keyParam of modelMeta.key_params) {
      instance[keyParam] = args.keyParams[keyParam] ?? instance[keyParam];
    }

    // Hydrate KV objects
    for (const kv of modelMeta.kv_objects) {
      const key = resolveKey(kv.format, instance, args.keyParams);
      if (
        (args.includeTree && args.includeTree[kv.value.name] === undefined) ||
        !key
      ) {
        if (kv.list_prefix) {
          instance[kv.value.name] = {
            results: [],
            cursor: null,
            complete: true,
          } as Paginated<KValue<unknown>>;
        }

        // Do not hydrate KV properties if they are not included in the include tree.
        // All keys must be resolved to perform hydration.
        continue;
      }

      const namespace: KVNamespace = args.env[kv.namespace_binding]!;
      if (kv.list_prefix) {
        args.promises.push(hydrateKVList(namespace, key, kv, instance));
      } else {
        args.promises.push(hydrateKVSingle(namespace, key, kv, instance));
      }
    }

    // Hydrate R2 objects
    for (const r2 of modelMeta.r2_objects) {
      const key = resolveKey(r2.format, instance, args.keyParams);
      if (
        (args.includeTree && args.includeTree[r2.var_name] === undefined) ||
        !key
      ) {
        if (r2.list_prefix) {
          instance[r2.var_name] = {
            results: [],
            cursor: null,
            complete: true,
          } as Paginated<R2ObjectBody>;
        }

        // Do not hydrate R2 properties if they are not included in the include tree.
        // All keys must be resolved to perform hydration.
        continue;
      }

      const bucket: R2Bucket = args.env[r2.bucket_binding]!;
      if (r2.list_prefix) {
        args.promises.push(
          (async () => {
            const list = await bucket.list({ prefix: key });

            const results = await Promise.all(
              list.objects.map(async (obj) => {
                const fullObj = await bucket.get(obj.key);
                return fullObj;
              }),
            );

            const cursor = list.truncated ? (list.cursor ?? null) : null;
            instance[r2.var_name] = {
              results,
              cursor,
              complete: !cursor,
            } as Paginated<R2ObjectBody>;
          })(),
        );
        continue;
      }

      args.promises.push(
        (async () => {
          const obj = await bucket.get(key);
          instance[r2.var_name] = obj;
        })(),
      );
    }

    return instance;
  }

  if (pooMeta) {
    for (const attr of pooMeta.attributes) {
      if (instance[attr.name] === undefined) {
        continue;
      }

      const res = hydrateType(instance[attr.name], attr.cidl_type, args);
      if (res) {
        instance[attr.name] = res;
      }
    }

    return instance;
  }
}

async function hydrateKVList(
  namespace: KVNamespace,
  key: string,
  kv: any,
  current: any,
) {
  const res = await namespace.list({ prefix: key });
  const cursor = !res.list_complete ? (res.cursor ?? null) : null;

  if (kv.value.cidl_type === "Stream") {
    const results = await Promise.all(
      res.keys.map(async (k: any) => {
        const stream = await namespace.get(k.name, { type: "stream" });
        return Object.assign(new KValue(), {
          key: k.name,
          raw: stream,
          metadata: null,
        });
      }),
    );

    current[kv.value.name] = {
      results,
      cursor,
      complete: res.list_complete || !cursor,
    } as Paginated<KValue<ReadableStream>>;
    return;
  }

  const results = await Promise.all(
    res.keys.map(async (k: any) => {
      const kvRes = await namespace.getWithMetadata(k.name, {
        type: "json",
      });
      return Object.assign(new KValue(), {
        key: k.name,
        raw: kvRes.value,
        metadata: kvRes.metadata,
      });
    }),
  );

  current[kv.value.name] = {
    results,
    cursor,
    complete: res.list_complete || !cursor,
  } as Paginated<KValue<unknown>>;
}

async function hydrateKVSingle(
  namespace: KVNamespace,
  key: string,
  kv: any,
  current: any,
) {
  if (kv.value.cidl_type === "Stream") {
    const res = await namespace.get(key, { type: "stream" });
    current[kv.value.name] = Object.assign(new KValue(), {
      key,
      raw: res,
      metadata: null,
    });

    return;
  }

  const res = await namespace.getWithMetadata(key, { type: "json" });
  current[kv.value.name] = Object.assign(new KValue(), {
    key,
    raw: res.value,
    metadata: res.metadata,
  });
}
