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
import {
  Model,
  CidlType,
  Cidl,
  getNavigationCidlType,
  KvR2Field,
} from "../cidl.js";
import { InternalError, u8ToB64 } from "../common.js";
import { DeepPartial, IncludeTree, KValue, Paginated } from "../ui/backend.js";

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
  static map<T extends object>(
    meta: Model,
    d1Results: D1Result,
    includeTree: IncludeTree<T>,
  ): T[] {
    const { wasm } = RuntimeContainer.get();
    const d1ResultsRes = WasmResource.fromString(
      JSON.stringify(d1Results.results),
      wasm,
    );
    const includeTreeRes = WasmResource.fromString(
      JSON.stringify(includeTree),
      wasm,
    );
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
    const includeTreeRes = WasmResource.fromString(
      JSON.stringify(includeTree),
      wasm,
    );
    const selectQueryRes = invokeOrmWasm(
      wasm.select_model,
      [WasmResource.fromString(meta.name, wasm), fromRes, includeTreeRes],
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
   * Given some base object (be it empty or the result of an `Orm.map`), hydrates all of the fields recursively
   * using the `includeTree` to determine which navigation properties to hydrate.
   *
   * For KV and R2 fields, performs the necessary queries to Workers KV and R2 to retrieve the data, which is then attached to the base object.
   */
  async hydrate<T extends object>(
    meta: Model,
    base: any,
    keyFields: Record<string, string>,
    includeTree: IncludeTree<T>,
  ): Promise<T> {
    base ??= {};
    const { ast } = RuntimeContainer.get();
    const modelCidlType: CidlType = {
      Object: { name: meta.name },
    };
    const env: any = this.env;
    const promises: Promise<void>[] = [];

    const hydrated = hydrateType(base, modelCidlType, {
      ast,
      includeTree,
      keyFields,
      env,
      promises,
    });

    await Promise.all(promises);
    return hydrated;
  }

  // TODO: Better ORM error handling strategies
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
  ): Promise<T | null> {
    includeTree ??= {} as IncludeTree<T>;
    const { wasm, ast } = RuntimeContainer.get();

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

    const db: D1Database | undefined = meta.d1_binding
      ? (this.env as any)[meta.d1_binding]
      : undefined;
    const queries = res.sql.map((s) => db!.prepare(s.query).bind(...s.values));

    // Concurrently execute SQL with KV uploads.
    const [batchRes] = await Promise.all([
      queries.length > 0 ? db!.batch(queries) : Promise.resolve([]),
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

      base = Orm.map(meta, batchRes[selectIndex!], includeTree)[0];
    }

    // Base needs to include all of the key fields from newModel and its includes.
    const q: Array<{ model: any; meta: Model; tree: IncludeTree<any> }> = [
      { model: base, meta, tree: includeTree },
    ]!;
    while (q.length > 0) {
      const {
        model: currentModel,
        meta: currentMeta,
        tree: currentTree,
      } = q.shift()!;

      // Key fields
      for (const keyField of currentMeta.key_fields) {
        if (currentModel[keyField] === undefined) {
          currentModel[keyField] = (newModel as any)[keyField];
        }
      }

      // Navigation properties
      for (const navProp of currentMeta.navigation_fields) {
        const nestedTree = currentTree[navProp.field.name];
        if (!nestedTree) continue;

        const nestedMeta = ast.models[navProp.model_reference];
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
    keyFields: Record<string, string>,
  ): Promise<T | null> {
    if (!query || !meta.d1_binding) {
      return await this.hydrate(meta, {}, keyFields, includeTree);
    }

    const res = await query.run();
    if (!res) {
      return null;
    }

    const mapped = Orm.map(meta, res, includeTree)[0];
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
  ): Promise<T[]> {
    if (!query || !meta.d1_binding) {
      // Not supported for non D1 models
      return [];
    }
    const rows = await query.run();
    const results = Orm.map(meta, rows, includeTree);
    await Promise.all(
      results.map(async (modelJson, index) => {
        results[index] = (await this.hydrate(
          meta,
          modelJson,
          {},
          includeTree,
        )) as T;
      }),
    );

    return results;
  }
}

/**
 * @returns null if any parameter could not be resolved
 */
function resolveKey(
  format: string,
  current: any,
  keyFields: Record<string, string>,
): string | null {
  try {
    return format.replace(/\{([^}]+)\}/g, (_, paramName) => {
      const paramValue = keyFields[paramName] ?? current[paramName];
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
    ast: Cidl;
    includeTree: IncludeTree<any> | null;
    keyFields: Record<string, string>;
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
      ? cidlType.Object.name
      : "Partial" in cidlType
        ? cidlType.Partial.object_name
        : null;
  if (objectName === null) {
    // Unsupported or unnecessary hydration.
    return value;
  }

  const modelMeta = args.ast.models[objectName];
  const pooMeta = args.ast.poos[objectName];
  const instance = value as any;

  if (modelMeta) {
    // Hydrate columns
    for (const col of modelMeta.columns) {
      if (instance[col.field.name] === undefined) {
        continue;
      }

      const res = hydrateType(
        instance[col.field.name],
        col.field.cidl_type,
        args,
      );
      if (res) {
        instance[col.field.name] = res;
      }
    }

    // Recursively hydrate navigation fields
    for (const nav of modelMeta.navigation_fields) {
      const tree = args.includeTree ? args.includeTree[nav.field.name] : null;
      if (tree === undefined) continue;

      const res = hydrateType(
        instance[nav.field.name],
        getNavigationCidlType(nav),
        {
          ...args,
          includeTree: tree,
        },
      );
      if (res) {
        instance[nav.field.name] = res;
      }
    }

    // Hydrate key fields
    for (const keyParam of modelMeta.key_fields) {
      instance[keyParam] = args.keyFields[keyParam] ?? instance[keyParam];
    }

    // Hydrate KV objects
    for (const kv of modelMeta.kv_fields) {
      const key = resolveKey(kv.format, instance, args.keyFields);
      if (
        (args.includeTree && args.includeTree[kv.field.name] === undefined) ||
        !key
      ) {
        if (kv.list_prefix) {
          instance[kv.field.name] = {
            results: [],
            cursor: null,
            complete: true,
          } as Paginated<KValue<unknown>>;
        }

        // Do not hydrate KV properties if they are not included in the include tree.
        // All keys must be resolved to perform hydration.
        continue;
      }

      const namespace: KVNamespace = args.env[kv.binding]!;
      if (kv.list_prefix) {
        args.promises.push(hydrateKVList(namespace, key, kv, instance));
      } else {
        args.promises.push(hydrateKVSingle(namespace, key, kv, instance));
      }
    }

    // Hydrate R2 objects
    for (const r2 of modelMeta.r2_fields) {
      const key = resolveKey(r2.format, instance, args.keyFields);
      if (
        (args.includeTree && args.includeTree[r2.field.name] === undefined) ||
        !key
      ) {
        if (r2.list_prefix) {
          instance[r2.field.name] = {
            results: [],
            cursor: null,
            complete: true,
          } as Paginated<R2ObjectBody>;
        }

        // Do not hydrate R2 properties if they are not included in the include tree.
        // All keys must be resolved to perform hydration.
        continue;
      }

      const bucket: R2Bucket = args.env[r2.binding]!;
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
            instance[r2.field.name] = {
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
          instance[r2.field.name] = obj;
        })(),
      );
    }

    return instance;
  }

  if (pooMeta) {
    for (const field of pooMeta.fields) {
      if (instance[field.name] === undefined) {
        continue;
      }

      const res = hydrateType(instance[field.name], field.cidl_type, args);
      if (res) {
        instance[field.name] = res;
      }
    }

    return instance;
  }
}

async function hydrateKVList(
  namespace: KVNamespace,
  key: string,
  kv: KvR2Field,
  current: any,
) {
  const res = await namespace.list({ prefix: key });
  const cursor = !res.list_complete ? (res.cursor ?? null) : null;

  if (kv.field.cidl_type === "Stream") {
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

    current[kv.field.name] = {
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

  current[kv.field.name] = {
    results,
    cursor,
    complete: res.list_complete || !cursor,
  } as Paginated<KValue<unknown>>;
}

async function hydrateKVSingle(
  namespace: KVNamespace,
  key: string,
  kv: KvR2Field,
  current: any,
) {
  if (kv.field.cidl_type === "Stream") {
    const res = await namespace.get(key, { type: "stream" });
    current[kv.field.name] = Object.assign(new KValue(), {
      key,
      raw: res,
      metadata: null,
    });

    return;
  }

  const res = await namespace.getWithMetadata(key, { type: "json" });
  current[kv.field.name] = Object.assign(new KValue(), {
    key,
    raw: res.value,
    metadata: res.metadata,
  });
}
