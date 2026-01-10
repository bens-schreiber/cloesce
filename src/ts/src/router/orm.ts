import type {
  ReadableStream,
  R2Bucket,
  D1Database,
  KVNamespace,
  D1Result,
} from "@cloudflare/workers-types";

import { DeepPartial, KValue, u8ToB64 } from "../ui/common.js";
import { RuntimeContainer } from "../router/router.js";
import { WasmResource, invokeOrmWasm } from "../router/wasm.js";
import { Model as AstModel } from "../ast.js";
import { InternalError } from "../common.js";
import { IncludeTree } from "../ui/backend.js";

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
   * @param includeTree Include tree specifying which navigation properties to include
   * @returns Array of mapped model instances
   */
  static map<T extends object>(
    ctor: new () => T,
    d1Results: D1Result,
    includeTree: IncludeTree<T> | null = null,
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
      [WasmResource.fromString(ctor.name, wasm), d1ResultsRes, includeTreeRes],
      wasm,
    );

    if (mapQueryRes.isLeft()) {
      throw new InternalError(`Mapping failed: ${mapQueryRes.value}`);
    }

    return JSON.parse(mapQueryRes.unwrap()) as T[];
  }

  // Given a model, generates a sequence of joins to select it with its includes.
  static select<T extends object>(
    ctor: new () => T,
    from: string | null = null,
    includeTree: IncludeTree<T> | null = null,
  ): string {
    const { wasm } = RuntimeContainer.get();
    const fromRes = WasmResource.fromString(JSON.stringify(from), wasm);
    const includeTreeRes = WasmResource.fromString(
      JSON.stringify(includeTree),
      wasm,
    );

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

  async hydrate<T extends object>(
    ctor: new () => T,
    args: {
      base?: any;
      keyParams?: Record<string, string>;
      includeTree?: IncludeTree<T> | null;
    } = {
      base: {},
      keyParams: {},
      includeTree: null,
    },
  ): Promise<T> {
    const { ast, constructorRegistry } = RuntimeContainer.get();
    const model = ast.models[ctor.name];
    if (!model) {
      return args.base ?? ({} as T);
    }
    const env: any = this.env;

    const instance: T = Object.assign(
      new constructorRegistry[model.name](),
      args.base,
    );
    const promises: Promise<void>[] = [];
    recurse(instance, model, args.includeTree ?? {});
    await Promise.all(promises);

    return instance;

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

    async function hydrateKVList(
      namespace: KVNamespace,
      key: string,
      kv: any,
      current: any,
    ) {
      const res = await namespace.list({ prefix: key });

      if (kv.value.cidl_type === "Stream") {
        current[kv.value.name] = await Promise.all(
          res.keys.map(async (k: any) => {
            const stream = await namespace.get(k.name, { type: "stream" });
            return {
              key: k.name,
              value: stream,
              raw: stream,
              metadata: null,
            } satisfies KValue<ReadableStream>;
          }),
        );
      } else {
        current[kv.value.name] = await Promise.all(
          res.keys.map(async (k: any) => {
            const kvRes = await namespace.getWithMetadata(k.name, {
              type: "json",
            });
            return {
              key: k.name,
              value: kvRes.value,
              raw: kvRes.value,
              metadata: kvRes.metadata,
            } satisfies KValue<unknown>;
          }),
        );
      }
    }

    async function hydrateKVSingle(
      namespace: KVNamespace,
      key: string,
      kv: any,
      current: any,
    ) {
      if (kv.value.cidl_type === "Stream") {
        const res = await namespace.get(key, { type: "stream" });
        current[kv.value.name] = {
          key,
          value: res,
          raw: res,
          metadata: null,
        } satisfies KValue<ReadableStream>;
      } else {
        const res = await namespace.getWithMetadata(key, { type: "json" });
        current[kv.value.name] = {
          key,
          value: res.value,
          raw: res.value,
          metadata: res.metadata,
        } satisfies KValue<unknown>;
      }
    }

    function recurse(
      current: any,
      meta: AstModel,
      includeTree: IncludeTree<any>,
    ) {
      // Hydrate navigation properties
      for (const navProp of meta.navigation_properties) {
        const nestedTree = includeTree[navProp.var_name];
        if (!nestedTree) continue;

        const nestedMeta = ast.models[navProp.model_reference];
        const value = current[navProp.var_name];

        if (Array.isArray(value)) {
          current[navProp.var_name] = value.map((child) => {
            const instance = Object.assign(
              new constructorRegistry[nestedMeta.name](),
              child,
            );
            recurse(instance, nestedMeta, nestedTree);
            return instance;
          });
        } else if (value) {
          current[navProp.var_name] = Object.assign(
            new constructorRegistry[nestedMeta.name](),
            value,
          );
          recurse(current[navProp.var_name], nestedMeta, nestedTree);
        }
      }

      // Hydrate columns
      for (const col of meta.columns) {
        if (current[col.value.name] === undefined) {
          continue;
        }

        switch (col.value.cidl_type) {
          case "DateIso": {
            current[col.value.name] = new Date(current[col.value.name]);
            break;
          }
          case "Blob": {
            const arr: number[] = current[col.value.name];
            current[col.value.name] = new Uint8Array(arr);
          }
          default: {
            break;
          }
        }
      }

      if (!args.keyParams) {
        return;
      }

      // Hydrate key params
      for (const keyParam of meta.key_params) {
        current[keyParam] = args.keyParams[keyParam];
      }

      // Hydrate KV objects
      for (const kv of meta.kv_objects) {
        // Include check
        if (includeTree[kv.value.name] === undefined) {
          if (kv.list_prefix) {
            current[kv.value.name] = [];
          }
          continue;
        }

        const key = resolveKey(kv.format, current, args.keyParams);
        if (!key) {
          if (kv.list_prefix) {
            current[kv.value.name] = [];
          }

          // All key params must be resolvable.
          // Fail silently by skipping hydration.
          continue;
        }

        const namespace: KVNamespace = env[kv.namespace_binding];

        if (kv.list_prefix) {
          promises.push(hydrateKVList(namespace, key, kv, current));
        } else {
          promises.push(hydrateKVSingle(namespace, key, kv, current));
        }
      }

      // Hydrate R2 objects
      for (const r2 of meta.r2_objects) {
        if (includeTree[r2.var_name] === undefined) {
          if (r2.list_prefix) {
            current[r2.var_name] = [];
          }
          continue;
        }

        const key = resolveKey(r2.format, current, args.keyParams);
        if (!key) {
          if (r2.list_prefix) {
            current[r2.var_name] = [];
          }

          // All key params must be resolvable.
          // Fail silently by skipping hydration.
          continue;
        }

        const bucket: R2Bucket = env[r2.bucket_binding];

        if (r2.list_prefix) {
          promises.push(
            (async () => {
              const list = await bucket.list({ prefix: key });

              current[r2.var_name] = await Promise.all(
                list.objects.map(async (obj) => {
                  const fullObj = await bucket.get(obj.key);
                  return fullObj;
                }),
              );
            })(),
          );
        } else {
          promises.push(
            (async () => {
              const obj = await bucket.get(key);
              current[r2.var_name] = obj;
            })(),
          );
        }
      }
    }
  }

  async upsert<T extends object>(
    ctor: new () => T,
    newModel: DeepPartial<T>,
    includeTree: IncludeTree<T> | null = null,
  ): Promise<T | null> {
    const { wasm } = RuntimeContainer.get();
    const upsertQueryRes = invokeOrmWasm(
      wasm.upsert_model,
      [
        WasmResource.fromString(ctor.name, wasm),
        WasmResource.fromString(
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
      throw new InternalError(`Upsert failed: ${upsertQueryRes.value}`);
    }

    const statements = JSON.parse(upsertQueryRes.unwrap()) as {
      query: string;
      values: any[];
    }[];

    // The SELECT statement towards the end will call `select_model` to retrieve the upserted model.
    let selectIndex: number;
    for (let i = statements.length - 1; i >= 0; i--) {
      if (/^SELECT/i.test(statements[i].query)) {
        selectIndex = i;
        break;
      }
    }

    // Execute all statements in a batch.
    const batchRes = await this.db.batch(
      statements.map((s) => this.db.prepare(s.query).bind(...s.values)),
    );

    const failed = batchRes.find((r) => !r.success);
    if (failed) {
      // An error in the upsert should not be possible unless the AST is invalid.
      throw new InternalError(
        `Upsert failed during execution: ${failed.error}`,
      );
    }

    const rootModelJson = Orm.map(ctor, batchRes[selectIndex!], includeTree)[0];

    // Hydrate and return the upserted model
    return await this.hydrate(ctor, {
      base: rootModelJson,
      includeTree,
    });
  }

  async list<T extends object>(
    ctor: new () => T,
    includeTree: IncludeTree<T> | null = null,
  ): Promise<T[]> {
    const { ast } = RuntimeContainer.get();
    const model = ast.models[ctor.name];
    if (!model) {
      return [];
    }

    if (model.primary_key === null) {
      // Listing is not supported for models without primary keys (i.e., KV or R2 only).
      return [];
    }

    const query = Orm.select(ctor, null, includeTree);
    const rows = await this.db.prepare(query).all();
    if (rows.error) {
      // An error in the query should not be possible unless the AST is invalid.
      throw new InternalError(
        `Failed to list models for ${ctor.name}: ${rows.error}`,
      );
    }

    // Map and hydrate
    const results = Orm.map(ctor, rows, includeTree);
    await Promise.all(
      results.map(async (modelJson, index) => {
        results[index] = await this.hydrate(ctor, {
          base: modelJson,
          includeTree,
        });
      }),
    );

    return results;
  }

  /**
   * Fetches a model by its primary key ID or key parameters.
   * * If the model does not have a primary key, key parameters must be provided.
   * * If the model has a primary key, the ID must be provided.
   *
   * @param ctor Constructor of the model to retrieve
   * @param args Arguments for retrieval
   * @returns The retrieved model instance, or `null` if not found
   */
  async get<T extends object>(
    ctor: new () => T,
    args: {
      id?: any;
      keyParams?: Record<string, string>;
      includeTree?: IncludeTree<T> | null;
    } = {
      id: undefined,
      keyParams: {},
      includeTree: null,
    },
  ): Promise<T | null> {
    const { ast } = RuntimeContainer.get();
    const model = ast.models[ctor.name];
    if (!model) {
      return null;
    }

    // KV or R2 only
    if (model.primary_key === null) {
      return await this.hydrate(ctor, {
        keyParams: args.keyParams,
        includeTree: args.includeTree ?? null,
      });
    }

    // D1 retrieval
    const pkName = model.primary_key.name;
    const query = `
      ${Orm.select(ctor, null, args.includeTree ?? null)}
      WHERE  "${model.name}"."${pkName}" = ?
    `;

    const rows = await this.db.prepare(query).bind(args.id).run();

    if (rows.error) {
      // An error in the query should not be possible unless the AST is invalid.
      throw new InternalError(
        `Failed to retrieve model ${ctor.name} with ${pkName}=${args.id}: ${rows.error}`,
      );
    }

    if (rows.results.length < 1) {
      return null;
    }

    // Map and hydrate
    const results = Orm.map(ctor, rows, args.includeTree ?? null);
    return await this.hydrate(ctor, {
      base: results[0],
      keyParams: args.keyParams,
      includeTree: args.includeTree ?? null,
    });
  }
}
