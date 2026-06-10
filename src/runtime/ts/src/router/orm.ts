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
  KvField,
  R2Field,
  isD1Backed,
  isDurableBacked,
  CONTEXT_INJECT_KEY,
} from "../cidl.js";
import { CloesceError, CloesceResult, InternalError, u8ToB64 } from "../common.js";
import { DeepPartial, IncludeTree, KValue, Paginated } from "../ui/backend.js";

type HydrateArgs = {
  idl: Cidl;
  includeTree: IncludeTree<any> | null;
  env: any;
  durable: DurableContext | null;
  promises: Promise<CloesceResult<void>>[];
};

type DurableContext = {
  state: {
    storage: {
      kv: { get(key: string): any; put(key: string, value: any): void };
    };
  };
};

interface KvRepository {
  /** Reads `key` as the single value or full page the field's type calls for. */
  hydrate(kv: KvField, key: string): Promise<KValue<unknown> | Paginated<KValue<unknown>>>;

  put(key: string, value: any, metadata?: unknown): Promise<void> | void;
}

interface R2Repository {
  /** Reads `key` as the single object or full page the field's type calls for. */
  hydrate(r2: R2Field, key: string): Promise<R2ObjectBody | null | Paginated<R2ObjectBody>>;
}

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
  private constructor(
    private env: any,
    private durable: DurableContext | null,
  ) {}

  static fromEnv(env: any): Orm {
    const durable = env[CONTEXT_INJECT_KEY] as DurableContext | undefined;
    return new Orm(env, durable ?? null);
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
      env,
      durable: this.durable,
      promises,
    });

    const errors = CloesceError.drain(await Promise.all(promises));
    if (errors) {
      return errors;
    }

    return { value: hydrated, errors: [] };
  }

  private kvRepositoryFor(meta: Model, binding: string): KvRepository {
    return kvRepositoryFor(meta, binding, { env: this.env, durable: this.durable });
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

    // Collect all KV uploads that can be performed concurrently with the SQL upsert.
    // Each upload is routed to its destination (a Worker KV namespace, or this model's
    // own Durable Object storage) by its binding.
    const kvUploadPromises = upsertRes.kv_uploads.map((upload) =>
      CloesceError.catchGeneric(async () => {
        const repo = this.kvRepositoryFor(meta, upload.namespace_binding);
        await repo.put(upload.key, upload.value, upload.metadata);
      }),
    );

    // The SQL upsert runs against the model's backing database. A DO-backed model's
    // SQLite store is reached through the DO context (no Worker D1 binding); that path
    // is a no-op until DO SQLite upserts land in a later phase.
    const db: D1Database | undefined = isD1Backed(meta)
      ? (this.env as any)[meta.backing!.binding]
      : undefined;
    const queries = db ? upsertRes.sql.map((s) => db.prepare(s.query).bind(...s.values)) : [];

    // Concurrently execute SQL with KV uploads.
    const [batchRes] = await Promise.all([
      queries.length > 0 ? db!.batch(queries) : Promise.resolve([]),
      ...kvUploadPromises,
    ]);

    let base: any = newModel;

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
      upsertRes.kv_delayed_uploads.map((upload): Promise<CloesceResult<void>> => {
        let current: any = base;
        for (const pathPart of upload.path) {
          current = current[pathPart];
          if (current === undefined) {
            throw new InternalError(
              `Failed to resolve path ${upload.path.join(".")} for delayed KV upload.`,
            );
          }
        }

        const key = resolveKey(upload.key, current);
        if (!key) {
          throw new InternalError(
            `Failed to resolve key format "${upload.key}" for delayed KV upload.`,
          );
        }

        const repo = this.kvRepositoryFor(meta, upload.namespace_binding);
        return CloesceError.catchGeneric(async () => {
          await repo.put(key, upload.value, upload.metadata);
        });
      }),
    );

    const delayedUploadErrors = CloesceError.drain(delayedUploadResults);
    if (delayedUploadErrors) {
      return delayedUploadErrors;
    }

    return await this.hydrate(meta, base, includeTree);
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
  ): Promise<CloesceResult<T | null>> {
    if (!query) {
      // No query provided, hydrate any non-SQL fields
      return await this.hydrate(meta, {}, includeTree);
    }

    const res = await query.run();
    if (!res.success) {
      return CloesceError.d1(res);
    }

    const mapped = Orm.map(meta, res, includeTree)[0];
    if (!mapped) {
      return { value: null, errors: [] };
    }
    return await this.hydrate(meta, mapped, includeTree);
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
    if (!query) {
      return { value: [], errors: [] };
    }

    const rows = await query.run();
    if (!rows.success) {
      return CloesceError.d1(rows);
    }

    const results = Orm.map(meta, rows, includeTree);
    const hydratedResults = await Promise.all(
      results.map((modelJson) => this.hydrate<T>(meta, modelJson, includeTree)),
    );

    const errors = hydratedResults.flatMap((r) => r.errors);
    if (errors.length > 0) {
      return { value: null, errors };
    }

    return { value: hydratedResults.map((r) => r.value as T), errors: [] };
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

      // A route model nav target has no unique fields: it is assembled entirely from
      // this model's route values, mapped onto the target's route fields in order.
      const target = args.idl.models[nav.model_reference];
      if (
        value[nav.field.name] === undefined &&
        !target.backing &&
        target.route_fields.length > 0 &&
        typeof nav.kind === "object" &&
        "OneToOne" in nav.kind
      ) {
        const columns = nav.kind.OneToOne.fields;
        const assembled: any = {};
        target.route_fields.forEach((field, i) => {
          assembled[field.name] = value[columns[i]];
        });
        value[nav.field.name] = assembled;
      }

      const res = hydrateType(value[nav.field.name], getNavigationCidlType(nav), {
        ...args,
        includeTree: tree,
      });
      if (res) value[nav.field.name] = res;
    }

    for (const kv of modelMeta.kv_fields) {
      const key = resolveKey(kv.key_format, value);
      if ((args.includeTree && args.includeTree[kv.field.name] === undefined) || !key) {
        const empty = emptyHydration(kv.field.cidl_type);
        if (empty) value[kv.field.name] = empty;
        continue;
      }

      const repo = kvRepositoryFor(modelMeta, kv.binding, args);
      args.promises.push(
        CloesceError.catchGeneric(async () => {
          value[kv.field.name] = await repo.hydrate(kv, key);
        }),
      );
    }

    for (const r2 of modelMeta.r2_fields) {
      const key = resolveKey(r2.key_format, value);
      if ((args.includeTree && args.includeTree[r2.field.name] === undefined) || !key) {
        const empty = emptyHydration(r2.field.cidl_type);
        if (empty) value[r2.field.name] = empty;
        continue;
      }
      const repo = r2RepositoryFor(r2.binding, args.env);
      args.promises.push(
        CloesceError.catchGeneric(async () => {
          value[r2.field.name] = await repo.hydrate(r2, key);
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

class WorkerKvRepository implements KvRepository {
  constructor(private namespace: KVNamespace) {}

  hydrate(kv: KvField, key: string): Promise<KValue<unknown> | Paginated<KValue<unknown>>> {
    return isPaginated(kv.field.cidl_type) ? this.list(kv, key) : this.get(kv, key);
  }

  private async get(kv: KvField, key: string): Promise<KValue<unknown>> {
    // KV Fields are capable of holding a `Stream` of data, which must be retrieved
    // with the `type: "stream"` option and cannot be JSON-parsed.
    //
    // A field is a `Stream` if the root type is of `Stream`.
    const isStreamField = (kv: KvField) => {
      const inner =
        typeof kv.field.cidl_type === "object" && "Paginated" in kv.field.cidl_type
          ? kv.field.cidl_type.Paginated
          : kv.field.cidl_type;
      return inner === "Stream";
    };

    if (isStreamField(kv)) {
      return new KValue(key, await this.namespace.get(key, { type: "stream" }), null);
    }

    const { value: raw, metadata } = await this.namespace.getWithMetadata(key, { type: "json" });
    return new KValue(key, raw, metadata);
  }

  private async list(kv: KvField, prefix: string): Promise<Paginated<KValue<unknown>>> {
    const res = await this.namespace.list({ prefix });
    const cursor = !res.list_complete ? (res.cursor ?? null) : null;
    const complete = res.list_complete || !cursor;

    const results = await Promise.all(res.keys.map((k: any) => this.get(kv, k.name)));
    return { results, cursor, complete };
  }

  put(key: string, value: any, metadata: unknown): Promise<void> {
    return this.namespace.put(key, JSON.stringify(value), { metadata });
  }
}

class DurableKvRepository implements KvRepository {
  constructor(private ctx: DurableContext) {}

  hydrate(kv: KvField, key: string): Promise<KValue<unknown> | Paginated<KValue<unknown>>> {
    return isPaginated(kv.field.cidl_type) ? this.list(kv, key) : this.get(kv, key);
  }

  private async get(_kv: KvField, key: string): Promise<KValue<unknown>> {
    const raw = this.ctx.state.storage.kv.get(key);
    return new KValue(key, raw ?? null, null);
  }

  // DO storage exposes only single-key gets; a prefix list resolves to the single
  // value at `prefix`.
  // TODO: SQL-backed DO storage will support true prefix scans.
  private async list(kv: KvField, prefix: string): Promise<Paginated<KValue<unknown>>> {
    const value = await this.get(kv, prefix);
    return {
      results: value.raw === null ? [] : [value],
      cursor: null,
      complete: true,
    };
  }

  put(key: string, value: any): void {
    this.ctx.state.storage.kv.put(key, value);
  }
}

class WorkerR2Repository implements R2Repository {
  constructor(private bucket: R2Bucket) {}

  hydrate(r2: R2Field, key: string): Promise<R2ObjectBody | null | Paginated<R2ObjectBody>> {
    return isPaginated(r2.field.cidl_type) ? this.list(key) : this.get(key);
  }

  private get(key: string): Promise<R2ObjectBody | null> {
    return this.bucket.get(key);
  }

  private async list(prefix: string): Promise<Paginated<R2ObjectBody>> {
    const res = await this.bucket.list({ prefix });
    const results = await Promise.all(res.objects.map((obj) => this.bucket.get(obj.key)));
    const cursor = res.truncated ? (res.cursor ?? null) : null;
    return { results, cursor, complete: !cursor } as Paginated<R2ObjectBody>;
  }
}

/**
 * @returns null if any parameter could not be resolved
 */
function resolveKey(format: string, current: any): string | null {
  try {
    return format.replace(/\{([^}]+)\}/g, (_, paramName) => {
      const paramValue = current[paramName];
      if (paramValue === undefined) throw null;
      return String(paramValue);
    });
  } catch {
    return null;
  }
}

/**
 * @returns a `KvRepository` instance based on the provided model metadata and binding,
 * routing to either a Worker KV namespace or Durable Object storage as appropriate.
 */
function kvRepositoryFor(
  meta: Model,
  binding: string,
  ctx: { env: any; durable: DurableContext | null },
): KvRepository {
  if (ctx.durable && isDurableBacked(meta) && binding === meta.backing!.binding) {
    return new DurableKvRepository(ctx.durable);
  }

  const namespace: KVNamespace | undefined = ctx.env?.[binding];
  if (!namespace) {
    throw new InternalError(`KV Namespace binding "${binding}" not found.`);
  }

  return new WorkerKvRepository(namespace);
}

/**
 * @returns an `R2Repository` instance based on the provided binding, routing to a Worker R2 bucket.
 */
function r2RepositoryFor(binding: string, env: any): R2Repository {
  const bucket: R2Bucket | undefined = env?.[binding];
  if (!bucket) {
    throw new InternalError(`R2 Bucket binding "${binding}" not found.`);
  }
  return new WorkerR2Repository(bucket);
}

function isPaginated(cidlType: CidlType): boolean {
  return typeof cidlType === "object" && "Paginated" in cidlType;
}

function emptyHydration(cidlType: CidlType): Paginated<never> | undefined {
  return isPaginated(cidlType) ? { results: [], cursor: null, complete: true } : undefined;
}
