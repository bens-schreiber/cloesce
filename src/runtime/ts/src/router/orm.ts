import type {
  R2Bucket,
  R2ObjectBody,
  D1Database,
  KVNamespace,
  D1Result,
  D1PreparedStatement,
} from "@cloudflare/workers-types";

import { RuntimeContainer } from "./router.js";
import { WasmResource, invokeOrmWasm } from "./wasm.js";
import { Model, CidlType, Cidl, getNavigationCidlType, KvField } from "../cidl.js";
import { CloesceError, CloesceResult, InternalError, u8ToB64 } from "../common.js";
import { DeepPartial, IncludeTree, KValue, Paginated } from "../ui/backend.js";

type HydrateArgs = {
  idl: Cidl;
  includeTree: IncludeTree<any> | null;
  keyFields: Record<string, unknown>;
  env: any;
  promises: Promise<CloesceResult<void>>[];
};

type KvUpload = {
  namespace_binding: string;
  key: string;
  value: any;
  metadata: unknown;
};

type UpsertResult = {
  sql: { query: string; values: any[] }[];
  kv_uploads: KvUpload[];
  kv_delayed_uploads: ({ path: string[] } & KvUpload)[];
};

export class Orm {
  private constructor(private env: unknown) {}

  static fromEnv(env: unknown): Orm {
    return new Orm(env);
  }

  /**
   * Maps a D1Result to a Cloesce Model based on the provided metadata and include tree.
   *
   * Fails silently if a mapping cannot be performed for some row, skipping it in results.
   *
   * Capable of mapping only queries returned from `Orm.select`, which are aliased in an Object Oriented fashion.
   */
  static map<T extends object>(meta: Model, d1Results: D1Result, includeTree: IncludeTree<T>): T[] {
    const { wasm } = RuntimeContainer.get();
    const d1ResultsRes = WasmResource.fromString(JSON.stringify(d1Results.results), wasm);
    const includeTreeRes = WasmResource.fromString(JSON.stringify(includeTree), wasm);
    const mapQueryRes = invokeOrmWasm(
      wasm.map,
      [WasmResource.fromString(meta.name, wasm), d1ResultsRes, includeTreeRes],
      wasm,
    );

    if (mapQueryRes.isLeft()) {
      throw new InternalError(`Mapping failed: ${mapQueryRes.value}`);
    }

    return JSON.parse(mapQueryRes.unwrap()) as T[];
  }

  /**
   * A SQL `SELECT` query generator based on the provided metadata and include tree.
   *
   * All navigation fields are `LEFT JOIN`ed to the result.
   * Aliases all fields of a Model in an Object Oriented fashion s.t. they can be referenced like:
   * `[navigation.name]`, `[navigation.nestedNavigation.name]`, etc. in the mapping function.
   *
   * All top level columns of a Model are aliased by name:
   * `[columnName]`.
   *
   * Utilizes the provided `includeTree` to determine which navigation fields to `LEFT JOIN` and include in the result.
   *
   * @returns a SQL `SELECT` string that can be executed to retrieve the Model(s).
   */
  static select<T extends object>(
    meta: Model,
    from: string | null,
    includeTree: IncludeTree<T>,
  ): string {
    if (!meta.d1_binding) {
      return "";
    }

    const { wasm } = RuntimeContainer.get();
    const fromRes = WasmResource.fromString(JSON.stringify(from ?? null), wasm);
    const includeTreeRes = WasmResource.fromString(JSON.stringify(includeTree), wasm);
    const selectQueryRes = invokeOrmWasm(
      wasm.select_model,
      [WasmResource.fromString(meta.name, wasm), fromRes, includeTreeRes],
      wasm,
    );

    if (selectQueryRes.isLeft()) {
      throw new InternalError(`Select generation failed: ${selectQueryRes.value}`);
    }

    return selectQueryRes.unwrap();
  }

  /**
   * Given some base object (be it empty or the result of an `Orm.map`), hydrates all of the fields recursively
   * using the `includeTree` to determine which navigation properties to hydrate.
   *
   * For KV and R2 fields, performs the necessary queries to Workers KV and R2 to retrieve the data, which is then attached to the base object.
   */
  async hydrate<T extends object>(
    meta: Model,
    base: any,
    keyFields: Record<string, unknown>,
    includeTree: IncludeTree<T>,
  ): Promise<CloesceResult<T>> {
    base ??= {};
    const { idl } = RuntimeContainer.get();
    const modelCidlType: CidlType = {
      Object: { name: meta.name },
    };
    const env: any = this.env;
    const promises: Promise<CloesceResult<void>>[] = [];

    const hydrated = hydrateType(base, modelCidlType, {
      idl,
      includeTree,
      keyFields,
      env,
      promises,
    });

    const errors = CloesceError.drain(await Promise.all(promises));
    if (errors) {
      return errors;
    }

    return { value: hydrated, errors: [] };
  }

  /**
   * Performs a SQL + KV upsert based on the provided metadata and include tree, returning the upserted model.
   * Recursively performs upserts for all nested navigation properties included in the `includeTree`.
   *
   * The `newModel` parameter is a partial model object that specifies the new values to upsert, and serves as the basis for generating the upsert SQL query.
   *
   * The SQL query will execute as an insert if:
   * - The primary key fields are missing from `newModel`
   *
   * The SQL query will execute as an insert OR update if:
   * - All fields are provided with values in `newModel`
   *
   * The SQL query will execute as an update if:
   * - At least one field is missing from `newModel`, but all primary key fields are provided with values.
   */
  async upsert<T extends object>(
    meta: Model,
    newModel: DeepPartial<T>,
    includeTree: IncludeTree<T>,
  ): Promise<CloesceResult<T | null>> {
    includeTree ??= {} as IncludeTree<T>;
    const { wasm, idl } = RuntimeContainer.get();

    // Invoke the ORM upsert function
    const upsertQueryRes = invokeOrmWasm(
      wasm.upsert_model,
      [
        WasmResource.fromString(meta.name, wasm),
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
      return CloesceError.cloesce(upsertQueryRes.value);
    }
    const upsertRes = JSON.parse(upsertQueryRes.unwrap()) as UpsertResult;

    // Collect all KV uploads that can be performed concurrently with the SQL upsert
    const kvUploadPromises = upsertRes.kv_uploads.map(async (upload) => {
      const namespace: KVNamespace | undefined = (this.env as any)[upload.namespace_binding];
      if (!namespace) {
        throw new InternalError(
          `KV Namespace binding "${upload.namespace_binding}" not found for upsert.`,
        );
      }

      return CloesceError.catchGeneric(
        async () =>
          await namespace.put(upload.key, JSON.stringify(upload.value), {
            metadata: upload.metadata,
          }),
      );
    });

    // If there are any SQL queries to execute, collect them as [D1PreparedStatement]s
    const db: D1Database | undefined = meta.d1_binding
      ? (this.env as any)[meta.d1_binding]
      : undefined;
    const queries = upsertRes.sql.map((s) => db!.prepare(s.query).bind(...s.values));

    // Concurrently execute SQL with KV uploads.
    const [batchRes] = await Promise.all([
      queries.length > 0 ? db!.batch(queries) : Promise.resolve([]),
      ...kvUploadPromises,
    ]);

    let base = {};

    // Ensure all queries succeeded, then map the result of the final SELECT statement to the Model
    if (queries.length > 0) {
      const failed = batchRes.find((r) => !r.success);
      if (failed) {
        return CloesceError.d1(failed);
      }

      let selectIndex = -1;
      for (let i = upsertRes.sql.length - 1; i >= 0; i--) {
        if (/^SELECT/i.test(upsertRes.sql[i].query)) {
          selectIndex = i;
          break;
        }
      }
      base = Orm.map(meta, batchRes[selectIndex], includeTree)[0];
    }

    // Base needs to include all of the key fields from newModel and its includes.
    const q: Array<{ model: any; meta: Model; tree: IncludeTree<any> }> = [
      { model: base, meta, tree: includeTree },
    ];
    while (q.length > 0) {
      const { model: currentModel, meta: currentMeta, tree: currentTree } = q.shift()!;

      // Key fields
      for (const field of currentMeta.key_fields) {
        if (currentModel[field.name] === undefined) {
          currentModel[field.name] = (newModel as any)[field.name];
        }
      }

      // Navigation properties
      for (const navProp of currentMeta.navigation_fields) {
        const nestedTree = currentTree[navProp.field.name];
        if (!nestedTree) continue;

        const nestedMeta = idl.models[navProp.model_reference];
        const value = currentModel[navProp.field.name];

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

    // Execute all delayed KV uploads that depended on SQL results
    const delayedUploadResults = await Promise.all(
      upsertRes.kv_delayed_uploads.map(async (upload): Promise<CloesceResult<void>> => {
        let current: any = base;
        for (const pathPart of upload.path) {
          current = current[pathPart];
          if (current === undefined) {
            throw new InternalError(
              `Failed to resolve path ${upload.path.join(".")} for delayed KV upload.`,
            );
          }
        }

        const namespace: KVNamespace | undefined = (this.env as any)[upload.namespace_binding];
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

        return CloesceError.catchGeneric(() =>
          namespace.put(key, JSON.stringify(upload.value), {
            metadata: upload.metadata,
          }),
        );
      }),
    );

    const delayedUploadErrors = CloesceError.drain(delayedUploadResults);
    if (delayedUploadErrors) {
      return delayedUploadErrors;
    }

    // Hydrate and return the upserted model
    return await this.hydrate(meta, base, {}, includeTree);
  }

  /**
   * Given some `D1PreparedStatement` that is expected to return a single row representing a Model,
   * maps the result to the Model based on the provided metadata and include tree, and returns it.
   *
   * See `Orm.map` for details on how the mapping is performed, and `Orm.hydrate` for details on how hydration is performed.
   */
  async get<T extends object>(
    meta: Model,
    query: D1PreparedStatement | null | undefined,
    includeTree: IncludeTree<T>,
    keyFields: Record<string, unknown>,
  ): Promise<CloesceResult<T | null>> {
    if (!query || !meta.d1_binding) {
      // No query provided, hydrate any non-SQL fields
      return await this.hydrate(meta, {}, keyFields, includeTree);
    }

    const res = await query.run();
    if (!res.success) {
      return CloesceError.d1(res);
    }

    const mapped = Orm.map(meta, res, includeTree)[0];
    if (!mapped) {
      return { value: null, errors: [] };
    }
    return await this.hydrate(meta, mapped, keyFields, includeTree);
  }

  /**
   * Given some `D1PreparedStatement` that is expected to return multiple rows representing Models,
   * maps the results to the Models based on the provided metadata and include tree, and returns them as an array.
   *
   * See `Orm.map` for details on how the mapping is performed, and `Orm.hydrate` for details on how hydration is performed.
   */
  async list<T extends object>(
    meta: Model,
    query: D1PreparedStatement | null | undefined,
    includeTree: IncludeTree<T>,
  ): Promise<CloesceResult<T[]>> {
    if (!query || !meta.d1_binding) {
      return { value: [], errors: [] };
    }

    const rows = await query.run();
    if (!rows.success) {
      return CloesceError.d1(rows);
    }

    const results = Orm.map(meta, rows, includeTree);
    const hydratedResults = await Promise.all(
      results.map((modelJson) => this.hydrate<T>(meta, modelJson, {}, includeTree)),
    );

    const errors = hydratedResults.flatMap((r) => r.errors);
    if (errors.length > 0) {
      return { value: null, errors };
    }

    return { value: hydratedResults.map((r) => r.value as T), errors: [] };
  }
}

/**
 * @returns null if any parameter could not be resolved
 */
function resolveKey(
  format: string,
  current: any,
  keyFields: Record<string, unknown>,
): string | null {
  try {
    return format.replace(/\{([^}]+)\}/g, (_, paramName) => {
      const paramValue = keyFields[paramName] ?? current[paramName];
      if (paramValue === undefined) throw null;
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
export function hydrateType(value: any, cidlType: CidlType, args: HydrateArgs): any | undefined {
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
      ? cidlType.Object.name
      : "Partial" in cidlType
        ? cidlType.Partial.object_name
        : null;
  if (objectName === null) {
    // Unsupported or unnecessary hydration.
    return value;
  }

  const modelMeta = args.idl.models[objectName];
  const pooMeta = args.idl.poos[objectName];

  if (modelMeta) {
    for (const col of modelMeta.columns) {
      if (value[col.field.name] === undefined) continue;
      const res = hydrateType(value[col.field.name], col.field.cidl_type, args);
      if (res) value[col.field.name] = res;
    }

    for (const nav of modelMeta.navigation_fields) {
      const tree = args.includeTree ? args.includeTree[nav.field.name] : null;
      if (tree === undefined) continue;
      const res = hydrateType(value[nav.field.name], getNavigationCidlType(nav), {
        ...args,
        includeTree: tree,
      });
      if (res) value[nav.field.name] = res;
    }

    for (const field of modelMeta.key_fields) {
      const raw = args.keyFields[field.name] ?? value[field.name];
      const hydrated = hydrateType(raw, field.cidl_type, args);
      value[field.name] = hydrated !== undefined ? hydrated : raw;
    }

    for (const kv of modelMeta.kv_fields) {
      const key = resolveKey(kv.format, value, args.keyFields);
      if ((args.includeTree && args.includeTree[kv.field.name] === undefined) || !key) {
        if (kv.list_prefix) {
          value[kv.field.name] = {
            results: [],
            cursor: null,
            complete: true,
          } as Paginated<KValue<unknown>>;
        }
        continue;
      }
      const namespace: KVNamespace = args.env[kv.binding]!;
      args.promises.push(
        kv.list_prefix
          ? hydrateKVList(namespace, key, kv, value)
          : hydrateKVSingle(namespace, key, kv, value),
      );
    }

    for (const r2 of modelMeta.r2_fields) {
      const key = resolveKey(r2.format, value, args.keyFields);
      if ((args.includeTree && args.includeTree[r2.field.name] === undefined) || !key) {
        if (r2.list_prefix) {
          value[r2.field.name] = {
            results: [],
            cursor: null,
            complete: true,
          } as Paginated<R2ObjectBody>;
        }
        continue;
      }
      const bucket: R2Bucket = args.env[r2.binding]!;
      args.promises.push(
        r2.list_prefix
          ? CloesceError.catchGeneric(async () => {
              const list = await bucket.list({ prefix: key });
              const results = await Promise.all(list.objects.map((obj) => bucket.get(obj.key)));
              const cursor = list.truncated ? (list.cursor ?? null) : null;
              value[r2.field.name] = {
                results,
                cursor,
                complete: !cursor,
              } as Paginated<R2ObjectBody>;
            })
          : CloesceError.catchGeneric(async () => {
              value[r2.field.name] = await bucket.get(key);
            }),
      );
    }

    return value;
  }

  if (pooMeta) {
    for (const field of pooMeta.fields) {
      if (value[field.name] === undefined) continue;
      const res = hydrateType(value[field.name], field.cidl_type, args);
      if (res) value[field.name] = res;
    }
    return value;
  }
}

function hydrateKVList(
  namespace: KVNamespace,
  key: string,
  kv: KvField,
  current: any,
): Promise<CloesceResult<void>> {
  return CloesceError.catchGeneric(async () => {
    const res = await namespace.list({ prefix: key });
    const cursor = !res.list_complete ? (res.cursor ?? null) : null;
    const complete = res.list_complete || !cursor;

    if (kv.field.cidl_type === "Stream") {
      const results = await Promise.all(
        res.keys.map(
          async (k: any) =>
            new KValue(k.name, await namespace.get(k.name, { type: "stream" }), null),
        ),
      );
      current[kv.field.name] = {
        results,
        cursor,
        complete,
      } as unknown as Paginated<KValue<ReadableStream>>;
      return;
    }

    const results = await Promise.all(
      res.keys.map(async (k: any) => {
        const { value: raw, metadata } = await namespace.getWithMetadata(k.name, { type: "json" });
        return new KValue(k.name, raw, metadata);
      }),
    );
    current[kv.field.name] = { results, cursor, complete } as Paginated<KValue<unknown>>;
  });
}

function hydrateKVSingle(
  namespace: KVNamespace,
  key: string,
  kv: KvField,
  current: any,
): Promise<CloesceResult<void>> {
  return CloesceError.catchGeneric(async () => {
    if (kv.field.cidl_type === "Stream") {
      current[kv.field.name] = new KValue(key, await namespace.get(key, { type: "stream" }), null);
      return;
    }
    const { value: raw, metadata } = await namespace.getWithMetadata(key, {
      type: "json",
    });
    current[kv.field.name] = new KValue(key, raw, metadata);
  });
}
