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
import Either from "../either.js";
import { IncludeTree } from "../ui/backend.js";

export class Orm {
  private constructor(private env: unknown) { }

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
   * Hydrates a base object into an instantiated object, including
   * navigation properties, KV objects, and R2 objects as defined
   * in the model AST.
   *
   * @param ctor The model constructor
   * @param base The base object to hydrate
   * @param keyParams Key parameters to assign during hydration
   * @param includeTree Include tree to define the relationships to hydrate.
   * @returns The hydrated model instance
   */
  async hydrate<T extends object>(
    ctor: new () => T,
    base: any,
    keyParams: Record<string, string>,
    includeTree: IncludeTree<T> | null = null,
  ): Promise<T> {
    const { ast, constructorRegistry } = RuntimeContainer.get();
    const model = ast.models[ctor.name];

    if (!model) {
      throw new Error(`Model ${ctor.name} not found in AST`);
    }

    const instance: T = Object.assign(
      new constructorRegistry[model.name](),
      base,
    );
    const promises: Promise<void>[] = [];
    const env: any = this.env;

    recurse(instance, model, includeTree ?? {});
    await Promise.all(promises);

    return instance;

    function resolveKey(format: string, current: any): string {
      return format.replace(/\{([^}]+)\}/g, (_, paramName) => {
        const paramValue = keyParams[paramName] ?? current[paramName];
        if (!paramValue) {
          throw new Error(
            `Parameter ${paramName} was missing during hydration`,
          );
        }
        return String(paramValue);
      });
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

    function recurse(current: any, meta: AstModel, tree: IncludeTree<any>) {
      // Hydrate navigation properties
      for (const navProp of meta.navigation_properties) {
        const nestedTree = tree[navProp.var_name];
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

      // Assign key params
      for (const keyParam of meta.key_params) {
        current[keyParam] = keyParams[keyParam];
      }

      // Hydrate KV objects
      for (const kv of meta.kv_objects) {
        if (tree[kv.value.name] === undefined) {
          if (kv.list_prefix) {
            current[kv.value.name] = [];
          }

          continue;
        }

        const key = resolveKey(kv.format, current);
        const namespace: KVNamespace = env[kv.namespace_binding];

        if (kv.list_prefix) {
          promises.push(hydrateKVList(namespace, key, kv, current));
        } else {
          promises.push(hydrateKVSingle(namespace, key, kv, current));
        }
      }

      // Hydrate R2 objects
      for (const r2 of meta.r2_objects) {
        if (tree[r2.var_name] === undefined) {
          if (r2.list_prefix) {
            current[r2.var_name] = [];
          }

          continue;
        }

        const key = resolveKey(r2.format, current);
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

  /**
   * Executes an "upsert" query, adding or augmenting a model in the database.
   *
   * If a model's primary key is not defined in `newModel`, the query is assumed to be an insert.
   *
   * If a model's primary key _is_ defined, but some attributes are missing, the query is assumed to be an update.
   *
   * Finally, if the primary key is defined, but all attributes are included, a SQLite upsert will be performed.
   *
   * In any other case, an  error will be thrown.
   *
   * ### Inserting a new Model
   * ```ts
   * const model = {name: "julio", lastname: "pumpkin"};
   * const idRes = await orm.upsert(Person, model, null);
   * ```
   *
   * ### Updating an existing model
   * ```ts
   * const model =  {id: 1, name: "timothy"};
   * const idRes = await orm.upsert(Person, model, null);
   * // (in db)=> {id: 1, name: "timothy", lastname: "pumpkin"}
   * ```
   *
   * ### Upserting a model
   * ```ts
   * // (assume a Person already exists)
   * const model = {
   *  id: 1,
   *  lastname: "burger", // updates last name
   *  dog: {
   *    name: "fido" // insert dog relationship
   *  }
   * };
   * const idRes = await orm.upsert(Person, model, null);
   * // (in db)=> Person: {id: 1, dogId: 1 ...}  ; Dog: {id: 1, name: "fido"}
   * ```
   *
   * @param ctor A model constructor.
   * @param newModel The new or augmented model.
   * @param includeTree An include tree describing which foreign keys to join.
   * @returns The primary key of the inserted model.
   */
  async upsert<T extends object>(
    ctor: new () => T,
    newModel: DeepPartial<T>,
    includeTree: IncludeTree<T> | null = null,
  ): Promise<Either<D1Result, any>> {
    const { wasm } = RuntimeContainer.get();
    const args = [
      WasmResource.fromString(ctor.name, wasm),
      WasmResource.fromString(
        JSON.stringify(newModel, (k, v) =>
          v instanceof Uint8Array ? u8ToB64(v) : v,
        ),
        wasm,
      ),
      WasmResource.fromString(JSON.stringify(includeTree), wasm),
    ];

    const upsertQueryRes = invokeOrmWasm(wasm.upsert_model, args, wasm);
    if (upsertQueryRes.isLeft()) {
      throw new Error(`Upsert failed internally: ${upsertQueryRes.value}`);
    }

    const statements = JSON.parse(upsertQueryRes.unwrap()) as {
      query: string;
      values: any[];
    }[];

    // One of these statements is a "SELECT", which is the root model id stmt.
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
      return Either.left(failed);
    }

    const rootModelId = batchRes[selectIndex!].results[0] as { id: any };
    return Either.right(rootModelId.id);
  }
}
