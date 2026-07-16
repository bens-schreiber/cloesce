import type { R2Bucket, R2ObjectBody, D1Database, KVNamespace } from "@cloudflare/workers-types";

import { RuntimeContainer } from "./router.js";
import { WasmResource, invokeOrmWasm } from "./wasm.js";
import { Model, CidlType, Cidl, getNavigationCidlType, ENV_DURABLE_TARGET_KEY } from "../cidl.js";
import { CloesceError, CloesceResult, Either, InternalError, u8ToB64 } from "../common.js";
import { DeepPartial, IncludeTree, KValue } from "../ui/backend.js";
import {
  executeSave,
  executeSelect,
  type KeyStore,
  type KeyValueWrapper,
  type SqlStore,
  type StorageResolver,
} from "./executor.js";
import type { Database, SavePlan, SelectPlan } from "./plan.js";

type HydrateArgs = {
  idl: Cidl;
  includeTree: IncludeTree<any> | null;
  env: any;
};

type DurableContext = {
  state: {
    storage: DurableStorage;
  };
};

type DurableStorage = {
  sql: {
    exec(query: string, ...bindings: any[]): { toArray(): Record<string, unknown>[] };
  };
  kv: {
    get(key: string): any;
    put(key: string, value: any): void;
    list(options?: { prefix?: string }): Iterable<[string, any]>;
  };
  transactionSync?<T>(closure: () => T): T;
};

export class Orm {
  private constructor(
    private env: any,
    private durable: DurableContext | null,
  ) {}

  static fromEnv(env: any): Orm {
    const durable = env[ENV_DURABLE_TARGET_KEY] as DurableContext | undefined;
    return new Orm(env, durable ?? null);
  }

  /**
   * Load a single `{@link Model}` (and its included relations) by its key params.
   *
   * `params` supplies every primary/route key the select plan requires.
   *
   * A precompiled `plan` (from `cidl.json`) skips the WASM planning call when supplied.
   */
  async get<T extends object>(
    meta: Model,
    params: Record<string, unknown>,
    includeTree: IncludeTree<T>,
    plan?: SelectPlan,
  ): Promise<CloesceResult<T | null>> {
    includeTree ??= {} as IncludeTree<T>;
    return this.runSelect(meta, "get", params, includeTree, plan, (body) => {
      return this.coerce(meta, body, includeTree) as T | null;
    });
  }

  /**
   * Load all matching `{@link Model}` rows (and included relations).
   *
   * - `params` carries the `limit` (and any shard/route keys) the list plan requires.
   * - A precompiled `plan` (from `cidl.json`) skips the WASM planning call when supplied.
   */
  async list<T extends object>(
    meta: Model,
    params: Record<string, unknown>,
    includeTree: IncludeTree<T>,
    plan?: SelectPlan,
  ): Promise<CloesceResult<T[]>> {
    includeTree ??= {} as IncludeTree<T>;
    return this.runSelect(meta, "list", params, includeTree, plan, (body) => {
      const rows = Array.isArray(body) ? body : [];
      return rows.map((row) => this.coerce(meta, row, includeTree) as T);
    });
  }

  /**
   * Plan (unless precompiled) and execute a select, shaping the hydrated body — which
   * may be partial when the executor sunk step errors. Anything thrown here is generic.
   */
  private async runSelect<R>(
    meta: Model,
    op: "get" | "list",
    params: Record<string, unknown>,
    includeTree: IncludeTree<any>,
    plan: SelectPlan | undefined,
    shape: (body: unknown) => R,
  ): Promise<CloesceResult<R>> {
    try {
      const selectPlan = plan ?? this.planSelect(meta, op, includeTree);
      const res = await executeSelect(
        selectPlan,
        params,
        this.storageResolver(),
        this.keyWrapper(meta, includeTree),
      );
      return { value: shape(res.value), errors: res.errors };
    } catch (e) {
      return CloesceError.generic(e);
    }
  }

  /**
   * Insert or update a `{@link Model}` and its included relations, returning the saved row
   * as the database's truth (in payload order).
   */
  async save<T extends object>(
    meta: Model,
    newModel: DeepPartial<T>,
    includeTree: IncludeTree<T>,
  ): Promise<CloesceResult<T | null>> {
    includeTree ??= {} as IncludeTree<T>;
    const planRes = this.planSave(meta, includeTree, newModel);
    if (planRes.isLeft()) {
      return CloesceError.cloesce(planRes.value);
    }
    try {
      const res = await executeSave(planRes.unwrap(), this.storageResolver());
      const body = res.value;
      return {
        value:
          body === null || body === undefined ? null : (this.coerce(meta, body, includeTree) as T),
        errors: res.errors,
      };
    } catch (e) {
      return CloesceError.generic(e);
    }
  }

  private planSelect(meta: Model, op: string, includeTree: IncludeTree<any>): SelectPlan {
    const { wasm } = RuntimeContainer.get();
    const res = invokeOrmWasm(
      wasm.plan_select,
      [
        WasmResource.fromString(meta.name, wasm),
        WasmResource.fromString(op, wasm),
        WasmResource.fromString(JSON.stringify(includeTree), wasm),
      ],
      wasm,
    );
    if (res.isLeft()) {
      throw new InternalError(`Select planning failed: ${res.value}`);
    }
    return JSON.parse(res.unwrap()) as SelectPlan;
  }

  private planSave(
    meta: Model,
    includeTree: IncludeTree<any>,
    payload: unknown,
  ): Either<string, SavePlan> {
    const { wasm } = RuntimeContainer.get();
    const res = invokeOrmWasm(
      wasm.plan_save,
      [
        WasmResource.fromString(meta.name, wasm),
        WasmResource.fromString(JSON.stringify(includeTree), wasm),
        WasmResource.fromString(
          // Serialize a Uint8Array so WASM can read it, as a base64 string.
          JSON.stringify(payload, (_, v) => (v instanceof Uint8Array ? u8ToB64(v) : v)),
          wasm,
        ),
      ],
      wasm,
    );
    return res.map((json) => JSON.parse(json) as SavePlan);
  }

  private storageResolver(): StorageResolver {
    const env = this.env;
    const durable = this.durable;
    return {
      sql(database: Database, shard: unknown[]): SqlStore {
        if (database.kind === "DurableObject") {
          return durableSqlStore(env, durable, database, shard);
        }
        return new D1SqlStore(env[database.name]);
      },
      key(database: Database, shard: unknown[]): KeyStore {
        switch (database.kind) {
          case "DurableObject":
            return durableKeyStore(env, durable, database, shard);
          case "R2":
            return new R2KeyStore(env, database.name);
          default:
            return new WorkerKvStore(env, database.name);
        }
      },
    };
  }

  /**
   * A read from a `Key` step is wrapped to match the field's declared type: Workers KV
   * fields become a {@link KValue}, DO-KV and R2 fields pass through raw (R2 already
   * returns an `R2ObjectBody`).
   */
  private keyWrapper(_meta: Model, _tree: IncludeTree<any>): KeyValueWrapper {
    return (database, _resultPath, raw, metadata) => {
      if (database.kind === "Kv") {
        // A Workers KV store returns `{ value, metadata }`; unwrap into a KValue.
        const inner = (raw ?? {}) as { value?: unknown; metadata?: unknown };
        return new KValue(inner.value ?? null, inner.metadata ?? metadata ?? null);
      }
      return raw ?? null;
    };
  }

  /**
   * Coerce a plan-produced body's scalar fields into their JS runtime types (Date, Uint8Array,
   * boolean) recursively.
   */
  private coerce(meta: Model, body: any, includeTree: IncludeTree<any>): any {
    const { idl } = RuntimeContainer.get();
    const modelCidlType: CidlType = { Object: { name: meta.name } };
    const hydrated = hydrateType(body, modelCidlType, {
      idl,
      includeTree,
      env: this.env,
    });
    return hydrated ?? body;
  }
}

class D1SqlStore implements SqlStore {
  constructor(private db: D1Database) {
    if (!db) {
      throw new InternalError("D1 database binding not found for a query plan.");
    }
  }

  async query(sql: string, bindings: unknown[]): Promise<Record<string, unknown>[]> {
    const res = await this.db
      .prepare(sql)
      .bind(...bindings.map(toSqlBind))
      .all();
    if (!res.success) {
      throw new InternalError(`D1 query failed: ${JSON.stringify(res)}`);
    }
    return res.results as Record<string, unknown>[];
  }

  async batch(
    statements: { sql: string; bindings: unknown[] }[],
  ): Promise<Record<string, unknown>[][]> {
    const prepared = statements.map((s) =>
      this.db.prepare(s.sql).bind(...s.bindings.map(toSqlBind)),
    );
    const results = await this.db.batch(prepared);
    const failed = results.find((r) => !r.success);
    if (failed) {
      throw new InternalError(`D1 batch failed: ${JSON.stringify(failed)}`);
    }
    return results.map((r) => (r.results ?? []) as Record<string, unknown>[]);
  }
}

/**
 * A DO's SQLite storage reached over RPC (from the Worker) or directly (inside the DO).
 * The Worker path routes to the stub identified by the shard tuple.
 */
function durableSqlStore(
  env: any,
  durable: DurableContext | null,
  database: Database,
  shard: unknown[],
): SqlStore {
  if (durable) {
    return new LocalDurableSqlStore(durable);
  }
  const stub = durableStub(env, database.name, shard);
  return new RemoteDurableSqlStore(stub);
}

/** Runs SQL directly against the DO context we are executing inside. */
class LocalDurableSqlStore implements SqlStore {
  constructor(private ctx: DurableContext) {}

  async query(sql: string, bindings: unknown[]): Promise<Record<string, unknown>[]> {
    return durableSqlBatch(this.ctx.state.storage, [{ sql, bindings }])[0];
  }

  async batch(
    statements: { sql: string; bindings: unknown[] }[],
  ): Promise<Record<string, unknown>[][]> {
    return durableSqlBatch(this.ctx.state.storage, statements);
  }
}

/** Runs SQL against a remote DO stub via the generated `__cloesceSqlBatch` RPC. */
class RemoteDurableSqlStore implements SqlStore {
  constructor(private stub: any) {}

  async query(sql: string, bindings: unknown[]): Promise<Record<string, unknown>[]> {
    const res = await this.stub.__cloesceSqlBatch([{ sql, bindings }]);
    return res[0];
  }

  async batch(
    statements: { sql: string; bindings: unknown[] }[],
  ): Promise<Record<string, unknown>[][]> {
    return await this.stub.__cloesceSqlBatch(statements);
  }
}

/**
 * @internal Runs an ordered SQL batch against a Durable Object's SQLite storage as a single
 * transaction, rolling back on the first failure. Returns each statement's rows.
 *
 * Exported for the generated DO subclass to expose as an RPC to the plan executor.
 */
export function durableSqlBatch(
  storage: DurableStorage,
  statements: { sql: string; bindings: unknown[] }[],
): Record<string, unknown>[][] {
  const runAll = () =>
    statements.map((s) => storage.sql.exec(s.sql, ...s.bindings.map(toSqlBind)).toArray());
  return storage.transactionSync ? storage.transactionSync(runAll) : runAll();
}

class WorkerKvStore implements KeyStore {
  private namespace: KVNamespace;

  constructor(env: any, binding: string) {
    this.namespace = env?.[binding];
    if (!this.namespace) {
      throw new InternalError(`KV Namespace binding "${binding}" not found.`);
    }
  }

  async get(key: string): Promise<unknown> {
    // Read value + metadata so a KValue can carry both.
    const { value, metadata } = await this.namespace.getWithMetadata(key, { type: "json" });
    return { value, metadata };
  }

  put(key: string, value: unknown, metadata?: unknown): Promise<void> {
    return this.namespace.put(key, JSON.stringify(value), { metadata: metadata as any });
  }
}

class R2KeyStore implements KeyStore {
  private bucket: R2Bucket;

  constructor(env: any, binding: string) {
    this.bucket = env?.[binding];
    if (!this.bucket) {
      throw new InternalError(`R2 Bucket binding "${binding}" not found.`);
    }
  }

  get(key: string): Promise<R2ObjectBody | null> {
    return this.bucket.get(key);
  }

  async put(key: string, value: unknown): Promise<void> {
    await this.bucket.put(key, value as any);
  }
}

function durableKeyStore(
  env: any,
  durable: DurableContext | null,
  database: Database,
  shard: unknown[],
): KeyStore {
  if (durable) {
    return new LocalDurableKeyStore(durable);
  }
  return new RemoteDurableKeyStore(durableStub(env, database.name, shard));
}

class LocalDurableKeyStore implements KeyStore {
  constructor(private ctx: DurableContext) {}

  get(key: string): unknown {
    return this.ctx.state.storage.kv.get(key) ?? null;
  }

  put(key: string, value: unknown): void {
    this.ctx.state.storage.kv.put(key, value);
  }
}

class RemoteDurableKeyStore implements KeyStore {
  constructor(private stub: any) {}

  async get(key: string): Promise<unknown> {
    return (await this.stub.__cloesceKvGet(key)) ?? null;
  }

  async put(key: string, value: unknown): Promise<void> {
    await this.stub.__cloesceKvPut(key, value);
  }
}

/** Resolve the stub for a DO shard from the raw shard values, mirroring the router's naming. */
function durableStub(env: any, binding: string, shard: unknown[]): any {
  const namespace = env[binding];
  if (!namespace) {
    throw new InternalError(`Durable Object binding "${binding}" not found.`);
  }
  const name = [binding, ...shard.map((v) => String(v))].join("/");
  return namespace.get(namespace.idFromName(name));
}

/** Coerce a JS value into a form the SQL drivers bind cleanly (booleans -> 0/1, blobs -> bytes). */
function toSqlBind(value: unknown): unknown {
  if (typeof value === "boolean") return value ? 1 : 0;
  if (value instanceof Uint8Array) return value;
  return value;
}

/**
 * @internal
 *
 * Coerces a plan-produced JSON value into its JS runtime type based on its CIDL type.
 * Recurses through columns, POO fields, and included navigations, but does **not** touch
 * KV/R2 fields (the plan already read and shaped them).
 *
 * @returns The coerced value if a replacement was necessary, or `undefined` if the value
 *   was mutated in place.
 */
export function hydrateType(value: any, cidlType: CidlType, args: HydrateArgs): any | undefined {
  if (value === null || value === undefined) {
    return value;
  }

  if (typeof cidlType === "object" && "Nullable" in cidlType) {
    // Unwrap nullable types
    cidlType = cidlType.Nullable;
  }

  if (typeof cidlType !== "object") {
    switch (cidlType) {
      case "DateIso": {
        return new Date(value);
      }
      case "Blob": {
        const arr = value as number[];
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
    for (const col of [...modelMeta.primary_columns, ...modelMeta.columns]) {
      if (value[col.field.name] === undefined) continue;
      const res = hydrateType(value[col.field.name], col.field.cidl_type, args);
      if (res !== undefined) value[col.field.name] = res;
    }

    for (const nav of modelMeta.navigation_fields) {
      if (nav.cardinality === "Many" && value[nav.field.name] === undefined) {
        // Default to an empty array for absent many navs.
        value[nav.field.name] = [];
      }

      const tree = args.includeTree ? args.includeTree[nav.field.name] : null;
      if (tree === undefined || value[nav.field.name] === undefined) {
        continue;
      }

      const res = hydrateType(value[nav.field.name], getNavigationCidlType(nav), {
        ...args,
        includeTree: tree,
      });
      if (res) value[nav.field.name] = res;
    }

    return value;
  }

  if (pooMeta) {
    for (const field of pooMeta.fields) {
      if (value[field.name] === undefined) continue;
      const res = hydrateType(value[field.name], field.cidl_type, args);
      if (res !== undefined) value[field.name] = res;
    }
    return value;
  }
}
