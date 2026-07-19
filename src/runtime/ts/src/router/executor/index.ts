/**
 * @internal
 * Runtime executor for the Cloesce query plan IR ({@link SelectPlan} / {@link SavePlan}).
 */

import type { CloesceErrorKind, CloesceResult } from "../../common.js";
import type { SelectPlan, SavePlan, Database, TemplateSegment } from "./plan.js";
import * as select from "./select.js";
import * as save from "./save.js";

export { MAX_BOUND_PARAMETERS } from "./select.js";

/**
 * Workers KV's bulk-read key budget per call.
 *
 * The bulk `get`/`getWithMetadata` overloads accept at most 100 keys per invocation (a
 * response over 25 MB fails with a 413), per
 * https://developers.cloudflare.com/kv/api/read-key-value-pairs/
 */
export const MAX_BULK_READ_KEYS = 100;

/** Split `values` into contiguous chunks of at most `size`. */
export function chunk<T>(values: T[], size: number): T[][] {
  const out: T[][] = [];
  for (let i = 0; i < values.length; i += size) {
    out.push(values.slice(i, i + size));
  }
  return out;
}

/** A SQL backend (D1 database or a DO shard's SQLite) a plan step queries. */
export interface SqlStore {
  query(sql: string, bindings: unknown[]): Promise<Record<string, unknown>[]>;

  /** Runs an ordered, transactional batch, returning each statement's rows. */
  batch(statements: { sql: string; bindings: unknown[] }[]): Promise<Record<string, unknown>[][]>;
}

/** A key-value backend (Workers KV, R2, or a DO shard's KV) */
export interface KeyStore {
  get(key: string): unknown;
  put(key: string, value: unknown, metadata?: unknown): unknown;
  getMany?(keys: string[]): Promise<Map<string, unknown>>;
}

/** Resolves a plan {@link Database} handle to a concrete store. */
export interface StorageResolver {
  sql(database: Database, shard: unknown[]): SqlStore;
  key(database: Database, shard: unknown[]): KeyStore;
}

/**
 * Execute a select plan.
 *
 * @param plan The select plan to execute.
 *  Precompiled or generated during runtime by the WASM module.
 *
 * @param params The parameters to bind to the plan's `Param` arguments.
 * @param storage The storage resolver to use for resolving plan databases.
 *
 * @param seed Optional initial data to seed the execution with.
 *  Execution will resolve around this data, skipping steps that would otherwise be required to fetch it.
 *
 * @returns A `CloesceResult` containing the final value and any errors encountered during execution.
 *  The result may be partial if some steps failed, or null if no root value was produced.
 */
export async function executeSelect(
  plan: SelectPlan,
  params: Record<string, unknown>,
  storage: StorageResolver,
  seed?: Record<string, unknown>[],
): Promise<CloesceResult<any>> {
  return select.execute(plan, storage, params, seed);
}

/**
 * Execute a save plan.
 *
 * @param plan The save plan to execute.
 *  Precompiled or generated during runtime by the WASM module.
 * @param storage The storage resolver to use for resolving plan databases.
 * @returns A `CloesceResult` containing the final value and any errors encountered during execution.
 *  The result may be partial if some steps failed, or null if no root value was produced.
 */
export async function executeSave(
  plan: SavePlan,
  storage: StorageResolver,
): Promise<CloesceResult<any>> {
  return save.execute(plan, storage);
}

/** Type a failed step's error by the storage it was targeting. */
export function stepError(database: Database | null, error: unknown): CloesceErrorKind {
  switch (database?.kind) {
    case "Kv":
      return { kind: "kv", error };
    case "R2":
      return { kind: "r2", error };
    default:
      return { kind: "generic", error };
  }
}

/** Wrap a body in a `CloesceResult` with the collected step errors. */
export function sinkResult(body: unknown, errors: CloesceErrorKind[]): CloesceResult<any> {
  return { value: body, errors };
}

/** Interpolate a key template from its already-resolved `Value` arguments, in order. */
export function interpolate<A>(segments: TemplateSegment<A>[], values: unknown[]): string {
  let next = 0;
  return segments.map((s) => ("Literal" in s ? s.Literal : keyText(values[next++]))).join("");

  function keyText(value: unknown): string {
    return typeof value === "string" ? value : JSON.stringify(value);
  }
}

/** The `Value` arguments of a key template, in interpolation order. */
export function templateArgs<A>(segments: TemplateSegment<A>[]): A[] {
  return segments.flatMap((s) => ("Value" in s ? [s.Value] : []));
}
