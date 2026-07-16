/**
 * @internal
 * Runtime executor for the Cloesce query plan IR ({@link SelectPlan} / {@link SavePlan}).
 *
 * A plan is a sequence of stages; every step within a stage is independent, so each
 * stage runs all of its step fetches concurrently. Results are then sunk sequentially
 * in step order (a pure in-memory pass), so a step attaching under a sibling's slot
 * always sees it. A failed step does not halt the plan: its error is collected into the
 * returned {@link CloesceResult} (typed by the step's storage kind), the step sinks a
 * blank result so dependents degrade to empty instead of cascading, and every later
 * stage still runs — a plan with many failures reports all of them, alongside the
 * partial body the surviving steps hydrated.
 */

import type {
  Database,
  JoinKeys,
  Mapping,
  PathSegment,
  SaveArg,
  SavePlan,
  SaveStep,
  SelectArg,
  SelectPlan,
  SelectStep,
  SqlStatement,
  TemplateSegment,
} from "./plan.js";
import type { CloesceErrorKind, CloesceResult } from "../common.js";

/** SQLite's bound-parameter budget per statement; spread expansions chunk to stay under it. */
export const MAX_BOUND_PARAMETERS = 100;

/** A SQL backend (D1 database or a DO shard's SQLite) a plan step queries. */
export interface SqlStore {
  query(sql: string, bindings: unknown[]): Promise<Record<string, unknown>[]>;

  /** Runs an ordered, transactional batch, returning each statement's rows. */
  batch(statements: { sql: string; bindings: unknown[] }[]): Promise<Record<string, unknown>[][]>;
}

/** A key-value backend (Workers KV, R2, or a DO shard's KV) a plan step reads/writes. */
export interface KeyStore {
  get(key: string): unknown;
  put(key: string, value: unknown, metadata?: unknown): unknown;
}

/** Resolves a plan {@link Database} handle (plus its DO shard tuple) to a concrete store. */
export interface StorageResolver {
  sql(database: Database, shard: unknown[]): SqlStore;
  key(database: Database, shard: unknown[]): KeyStore;
}

/** Shapes a raw `Key` step read into the value placed at the step's result path. */
export type KeyValueWrapper = (
  database: Database,
  resultPath: string[],
  raw: unknown,
  metadata?: unknown,
) => unknown;

/**
 * Execute a select plan; the value is the hydrated body (object, array, or null).
 *
 * When `seed` is supplied its rows become the root table and every plan step whose
 * result path the seed covers is skipped (see {@link select.Execution.plantSeeds}): the
 * caller pre-sources part of the result and the executor hydrates only the rest. Seeds
 * are consumed, their objects stamped and attached to in place rather than cloned.
 */
export async function executeSelect(
  plan: SelectPlan,
  params: Record<string, unknown>,
  storage: StorageResolver,
  wrap: KeyValueWrapper,
  seed?: Record<string, unknown>[],
): Promise<CloesceResult<any>> {
  return new select.Execution(params, storage, wrap, seed).run(plan);
}

/** Execute a save plan; the value is the saved body as the database's truth. */
export async function executeSave(
  plan: SavePlan,
  storage: StorageResolver,
): Promise<CloesceResult<any>> {
  return new save.Execution(storage).run(plan);
}

/** Type a failed step's error by the storage it was targeting. */
function stepError(database: Database | null, error: unknown): CloesceErrorKind {
  switch (database?.kind) {
    case "Kv":
      return { kind: "kv", error };
    case "R2":
      return { kind: "r2", error };
    default:
      return { kind: "generic", error };
  }
}

/** Wrap a (possibly partial) body in a `CloesceResult` with the collected step errors. */
function sinkResult(body: unknown, errors: CloesceErrorKind[]): CloesceResult<any> {
  return { value: body, errors };
}

/** Render one segment value of a KV/R2 key template. */
function keyText(value: unknown): string {
  return typeof value === "string" ? value : JSON.stringify(value);
}

/** Interpolate a key template from its already-resolved `Value` arguments, in order. */
function renderKey<A>(segments: TemplateSegment<A>[], values: unknown[]): string {
  let next = 0;
  return segments.map((s) => ("Literal" in s ? s.Literal : keyText(values[next++]))).join("");
}

/** The `Value` arguments of a key template, in interpolation order. */
function templateArgs<A>(segments: TemplateSegment<A>[]): A[] {
  return segments.flatMap((s) => ("Value" in s ? [s.Value] : []));
}

namespace select {
  /** A hydrated value and the index of the parent-table row it belongs to. */
  interface Attachment {
    parent: number;
    value: any;
  }

  /**
   * The hydrated output of one step: every value it produced, each tied back to a row
   * of its parent table. Tables live keyed by result path until {@link Execution.assemble}
   * folds children into their parents.
   */
  interface Table {
    attachments: Attachment[];
    many: boolean;
  }

  /** A {@link SelectArg} split into its owner table path and field. */
  type Arg = { kind: "param"; name: string } | { kind: "field"; owner: string[]; field: string };

  function arg(raw: SelectArg): Arg {
    if ("Param" in raw) return { kind: "param", name: raw.Param };
    const path = raw.Result;
    return { kind: "field", owner: path.slice(0, -1), field: path[path.length - 1] };
  }

  type Fetched =
    | { kind: "rows"; rows: Record<string, unknown>[] }
    /** Key reads by their resolved argument tuple (JSON-encoded). */
    | { kind: "keys"; values: Map<string, unknown> }
    | { kind: "none" };

  /** The storage a step targets, for error typing; Synthesize steps touch none. */
  function database(step: SelectStep): Database | null {
    const q = step.query;
    if ("Sql" in q) return q.Sql.database;
    if ("Key" in q) return q.Key.database;
    return null;
  }

  export class Execution {
    /** Hydrated tables of completed steps, keyed by JSON-encoded result path. */
    private tables = new Map<string, Table>();

    /** Result paths whose table a seed pre-supplied; their steps are not fetched. */
    private seeded = new Set<string>();

    constructor(
      private params: Record<string, unknown>,
      private storage: StorageResolver,
      private wrap: KeyValueWrapper,
      private seed?: Record<string, unknown>[],
    ) {}

    async run(plan: SelectPlan): Promise<CloesceResult<any>> {
      const errors: CloesceErrorKind[] = [];
      if (this.seed) {
        try {
          this.plantSeeds(plan);
        } catch (e) {
          // A malformed seed (non-array Many field) aborts hydration; surface
          // it as a collected error rather than an uncaught throw.
          return sinkResult(null, [stepError(null, e)]);
        }
      }
      for (const stage of plan.stages) {
        const pending = stage.steps.filter((s) => !this.seeded.has(JSON.stringify(s.result)));
        const settled = await Promise.allSettled(pending.map((s) => this.fetch(s)));
        settled.forEach((res, i) => {
          const step = pending[i];
          try {
            if (res.status === "fulfilled") this.attach(step, res.value);
            else throw res.reason;
          } catch (e) {
            errors.push(stepError(database(step), e));
            this.attachBlank(step);
          }
        });
      }
      return sinkResult(this.assemble(), errors);
    }

    /**
     * Pre-populate tables from caller-supplied seed rows so {@link run} skips the
     * steps they cover. The root rows are planted directly, then every step is walked
     * in plan order:
     *
     * - A step is seeded when its parent path is already seeded and its field is
     *   present (`!== undefined`) on at least one parent attachment.
     * - Parents lacking the field contribute no attachments, so they degrade to
     *   empty. Missing data is never an error.
     * - Each seeded step also gets its derived (shard, route, synthesized) fields
     *   stamped, mirroring a live fetch so unseeded descendants still route and join.
     *
     * Seeds are consumed in place: their objects are stamped and attached to, never cloned.
     */
    private plantSeeds(plan: SelectPlan): void {
      this.tables.set(JSON.stringify([]), {
        attachments: this.seed!.map((value) => ({ parent: 0, value })),
        many: true,
      });
      this.seeded.add(JSON.stringify([]));

      for (const stage of plan.stages) {
        for (const step of stage.steps) {
          const path = step.result;
          const parentSeeded = this.seeded.has(JSON.stringify(path.slice(0, -1)));
          const seededHere = this.seeded.has(JSON.stringify(path));
          if (path.length === 0) {
            this.stampDerived(step);
          } else if (parentSeeded && !seededHere && this.seedChild(step)) {
            this.stampDerived(step);
          }
        }
      }
    }

    /**
     * Build a child step's table from the field the seed supplied on its parent.
     *
     * - Returns `false` (leaving the step to be fetched) when the field is absent on
     *   every parent.
     * - `Many` fields flatten their array elements into attachments (`[]` seeds empty);
     *   `One`/`Key` fields attach one non-null value. A `null`, or an absent parent,
     *   contributes nothing.
     */
    private seedChild(step: SelectStep): boolean {
      const path = step.result;
      const field = path[path.length - 1];
      const parent = this.table(path.slice(0, -1))!;
      if (parent.attachments.every((a) => a.value[field] === undefined)) return false;

      const q = step.query;
      const many =
        "Sql" in q
          ? q.Sql.mapping.cardinality === "Many"
          : "Synthesize" in q
            ? q.Synthesize.cardinality === "Many"
            : false;
      const attachments: Attachment[] = [];
      parent.attachments.forEach((a, idx) => {
        const value = a.value[field];
        if (value === undefined || value === null) return;
        if (many) {
          if (!Array.isArray(value)) {
            throw new Error(
              `seed at ${JSON.stringify(path)}: Many field "${field}" must be an array`,
            );
          }
          for (const el of value) attachments.push({ parent: idx, value: el });
        } else {
          attachments.push({ parent: idx, value });
        }
      });
      this.tables.set(JSON.stringify(path), { attachments, many });
      this.seeded.add(JSON.stringify(path));
      return true;
    }

    /**
     * Fill a seeded step's derived fields where the seed left them undefined, so the
     * seed matches what a live fetch would have produced. Covers an `Sql` step's shard
     * and route fields and a `Synthesize` step's fields. `Param` args resolve from
     * params, `Result` args off the parent attachment chain. Seed values always win.
     */
    private stampDerived(step: SelectStep): void {
      const q = step.query;
      const fields =
        "Sql" in q
          ? [...(q.Sql.shard ?? []), ...(q.Sql.route_fields ?? [])]
          : "Synthesize" in q
            ? q.Synthesize.fields
            : [];
      if (fields.length === 0) return;

      const table = this.table(step.result)!;
      table.attachments.forEach((att, idx) => {
        for (const [field, raw] of fields) {
          if (att.value[field] !== undefined) continue;
          const value = this.resolveFrom(step.result, idx, arg(raw));
          if (value !== undefined) att.value[field] = value;
        }
      });
    }

    private async fetch(step: SelectStep): Promise<Fetched> {
      const q = step.query;
      if ("Sql" in q) return { kind: "rows", rows: await this.fetchSql(step, q.Sql) };
      if ("Key" in q) return { kind: "keys", values: await this.fetchKeys(step, q.Key) };
      return { kind: "none" };
    }

    private async fetchSql(
      step: SelectStep,
      q: {
        database: Database;
        sql: string;
        arguments: SelectArg[];
        shard: [string, SelectArg][];
        route_fields?: [string, SelectArg][];
      },
    ): Promise<Record<string, unknown>[]> {
      // A `Param` binds one value; a `Result` spreads the distinct values of its owner
      // table's field. A spread over zero values selects nothing.
      const args = q.arguments.map(arg);
      const spreads = args.map((a) => this.spread(a, step.result));
      if (spreads.some((s) => s.length === 0)) return [];
      const statements = chunkStatements(q.sql, args, spreads);

      const shardArgs = (q.shard ?? []).map(([, a]) => arg(a));
      const shardTuples = this.tuples(shardArgs, step.result);

      // Param route fields are constant across shards; field-sourced route fields are
      // deferred to the sink, where each row's parent is known.
      const constants = (q.route_fields ?? []).flatMap(([field, raw]) => {
        const a = arg(raw);
        return a.kind === "param" ? [[field, this.param(a.name)] as const] : [];
      });

      const perShard = await Promise.all(
        shardTuples.map(async (tuple) => {
          const store = this.storage.sql(q.database, tuple);
          const results =
            statements.length === 1
              ? [await store.query(statements[0].sql, statements[0].bindings)]
              : await store.batch(statements);
          // Stamp shard fields so joins and deeper shards can read them off the row.
          const stamps = [...q.shard.map(([field], i) => [field, tuple[i]] as const), ...constants];
          return results.flat().map((row) => {
            for (const [field, value] of stamps) row[field] = value;
            return row;
          });
        }),
      );
      return perShard.flat();
    }

    /** Read from key stores: one fetch per distinct argument tuple, all concurrent. */
    private async fetchKeys(
      step: SelectStep,
      q: {
        database: Database;
        segments: TemplateSegment<SelectArg>[];
        shard: [string, SelectArg][];
      },
    ): Promise<Map<string, unknown>> {
      const args = keyArgs(q.shard, q.segments);
      const tuples = this.tuples(args, step.result);
      const entries = await Promise.all(
        tuples.map(async (tuple) => {
          const shard = tuple.slice(0, q.shard.length);
          const key = renderKey(q.segments, tuple.slice(q.shard.length));
          const raw = await this.storage.key(q.database, shard).get(key);
          return [JSON.stringify(tuple), this.wrap(q.database, step.result, raw)] as const;
        }),
      );
      return new Map(entries);
    }

    private table(path: string[]): Table | undefined {
      return this.tables.get(JSON.stringify(path));
    }

    private param(name: string): unknown {
      if (!(name in this.params)) throw new Error(`missing parameter "${name}"`);
      return this.params[name];
    }

    /**
     * Resolve an argument for the object at (`path`, `idx`), climbing the parent
     * back-refs up to the argument's owner table. `undefined` when a value is absent.
     */
    private resolveFrom(path: string[], idx: number, a: Arg): unknown {
      if (a.kind === "param") return this.param(a.name);
      let cur = path;
      while (cur.length > a.owner.length) {
        idx = this.table(cur)?.attachments[idx]?.parent ?? 0;
        cur = cur.slice(0, -1);
      }
      return this.table(cur)?.attachments[idx]?.value?.[a.field];
    }

    /**
     * The distinct value tuples of `args`, one per hydrated object of the deepest owner
     * table any argument references (a single tuple when every arg is a param). An arg
     * owned by an ancestor of that table resolves by climbing the parent back-refs.
     * Tuples with a missing value are skipped.
     */
    private tuples(args: Arg[], at: string[]): unknown[][] {
      const owners = args.flatMap((a) => (a.kind === "field" ? [a.owner] : []));
      if (owners.length === 0) return [args.map((a) => this.resolveFrom([], 0, a))];

      const deepest = owners.reduce((a, b) => (b.length > a.length ? b : a));
      if (!owners.every((o) => o.every((seg, i) => deepest[i] === seg))) {
        throw new Error(
          `select step at ${JSON.stringify(at)}: arguments span unrelated tables (plan IR defect)`,
        );
      }
      const table = this.table(deepest);
      if (!table) {
        // A same-stage or missing owner table would race the fetch: a plan IR defect.
        throw new Error(
          `select step at ${JSON.stringify(at)} reads table ${JSON.stringify(deepest)} ` +
            `before an earlier stage produced it (plan IR defect)`,
        );
      }

      const seen = new Set<string>();
      const out: unknown[][] = [];
      table.attachments.forEach((_, idx) => {
        const tuple = args.map((a) => this.resolveFrom(deepest, idx, a));
        if (tuple.some((v) => v === undefined)) return;
        const key = JSON.stringify(tuple);
        if (!seen.has(key)) {
          seen.add(key);
          out.push(tuple);
        }
      });
      return out;
    }

    /** The distinct values one argument spreads to at fetch time. */
    private spread(a: Arg, at: string[]): unknown[] {
      return this.tuples([a], at).map(([v]) => v);
    }

    private attach(step: SelectStep, fetched: Fetched): void {
      const q = step.query;
      const key = JSON.stringify(step.result);
      if ("Sql" in q && fetched.kind === "rows") {
        this.tables.set(key, this.rowTable(step.result, q.Sql, fetched.rows));
      } else if ("Key" in q && fetched.kind === "keys") {
        const args = keyArgs(q.Key.shard, q.Key.segments);
        const parentPath = step.result.slice(0, -1);
        this.tables.set(
          key,
          this.singletons(step.result, (idx) => {
            const tuple = args.map((a) => this.resolveFrom(parentPath, idx, a));
            return fetched.values.get(JSON.stringify(tuple)) ?? null;
          }),
        );
      } else if ("Synthesize" in q) {
        this.synthesize(step.result, q.Synthesize);
      }
    }

    /**
     * Sink a failed step as an empty table (unless a sibling already produced one at the
     * path), so steps of later stages that read it degrade to empty instead of failing.
     */
    private attachBlank(step: SelectStep): void {
      const key = JSON.stringify(step.result);
      if (this.tables.has(key)) return;
      const q = step.query;
      const many = "Sql" in q && q.Sql.mapping.cardinality === "Many";
      this.tables.set(key, { attachments: [], many });
    }

    /** Tie each fetched row to its parents via the mapping's join keys. */
    private rowTable(
      path: string[],
      q: { mapping: Mapping; route_fields?: [string, SelectArg][] },
      rows: Record<string, unknown>[],
    ): Table {
      const many = q.mapping.cardinality === "Many";
      if (path.length === 0) {
        return { attachments: rows.map((value) => ({ parent: 0, value })), many };
      }

      const parentPath = path.slice(0, -1);
      const parent = this.table(parentPath);
      if (!parent) return { attachments: [], many };

      // Bucket parents by join key, then hand each row to its matching parents;
      // a One mapping serves each parent at most once.
      const joinKey = (row: any, side: (j: JoinKeys) => string) =>
        JSON.stringify(q.mapping.join.map((j) => row[side(j)] ?? null));
      const index = new Map<string, number[]>();
      parent.attachments.forEach((a, i) => {
        const k = joinKey(a.value, (j) => j.parent_key);
        const bucket = index.get(k);
        if (bucket) {
          bucket.push(i);
        } else {
          index.set(k, [i]);
        }
      });

      const routeFields = (q.route_fields ?? [])
        .map(([field, raw]) => [field, arg(raw)] as const)
        .filter(([, a]) => a.kind === "field");

      const served = new Set<number>();
      const attachments: Attachment[] = [];
      for (const row of rows) {
        for (const p of index.get(joinKey(row, (j) => j.child_key)) ?? []) {
          if (!many) {
            if (served.has(p)) continue;
            served.add(p);
          }
          // Clone: a row may attach under several parents, each hydrated independently.
          const value: Record<string, unknown> = { ...row };
          for (const [field, a] of routeFields) {
            value[field] = this.resolveFrom(parentPath, p, a);
          }
          attachments.push({ parent: p, value });
        }
      }
      return { attachments, many };
    }

    /**
     * Materialize or merge a synthesized object. When a table already exists at the path
     * (produced by an earlier step), fields merge onto each of its values; otherwise a
     * fresh singleton is built per parent.
     */
    private synthesize(
      path: string[],
      q: { fields: [string, SelectArg][]; cardinality: "One" | "Many" },
    ): void {
      const parentPath = path.slice(0, -1);
      const fields = q.fields.map(([field, raw]) => [field, arg(raw)] as const);

      const existing = this.table(path);
      if (existing) {
        for (const att of existing.attachments) {
          for (const [field, a] of fields) {
            att.value[field] = this.resolveFrom(parentPath, att.parent, a);
          }
        }
        return;
      }

      // A Many synthesize is a singleton array, folded as a single value.
      // At the root every field is param-sourced by construction.
      this.tables.set(
        JSON.stringify(path),
        this.singletons(path, (idx) => {
          const object = Object.fromEntries(
            fields.map(([field, a]) => [field, this.resolveFrom(parentPath, idx, a)]),
          );
          return q.cardinality === "One" ? object : [object];
        }),
      );
    }

    /** A table with exactly one value per parent row (one root value when `path` is root). */
    private singletons(path: string[], build: (parentIdx: number) => unknown): Table {
      if (path.length === 0) {
        return { attachments: [{ parent: 0, value: build(0) }], many: false };
      }
      const parent = this.table(path.slice(0, -1));
      const attachments = (parent?.attachments ?? []).map((_, i) => ({
        parent: i,
        value: build(i),
      }));
      return { attachments, many: false };
    }

    /** Fold every table into its parent, deepest first, producing the final body. */
    private assemble(): any {
      const keys = [...this.tables.keys()].sort(
        (a, b) => (JSON.parse(b) as string[]).length - (JSON.parse(a) as string[]).length,
      );
      for (const key of keys) {
        const path = JSON.parse(key) as string[];
        if (path.length === 0) break;
        const child = this.tables.get(key)!;
        this.tables.delete(key);

        const field = path[path.length - 1];
        const parent = this.table(path.slice(0, -1));
        if (!parent) continue;
        if (child.many) {
          for (const a of parent.attachments) a.value[field] = [];
        }
        for (const att of child.attachments) {
          const slot = parent.attachments[att.parent];
          if (!slot) continue;
          if (child.many) slot.value[field].push(att.value);
          else slot.value[field] = att.value;
        }
      }

      const root = this.tables.get(JSON.stringify([]));
      if (!root) return null;
      const values = root.attachments.map((a) => a.value);
      return root.many ? values : (values[0] ?? null);
    }
  }

  /** A key step's arguments in tuple order: shard values, then template values. */
  function keyArgs(shard: [string, SelectArg][], segments: TemplateSegment<SelectArg>[]): Arg[] {
    return [...shard.map(([, a]) => a), ...templateArgs(segments)].map(arg);
  }

  /**
   * Expand a SQL statement's spread placeholders into concrete `(sql, bindings)` chunks.
   * Every chunk stays within {@link MAX_BOUND_PARAMETERS}: fixed params are re-bound in
   * each, and spread values split the remaining budget evenly. Multiple spreads chunk as
   * a cross product so every value combination is covered exactly once.
   */
  function chunkStatements(
    sql: string,
    args: Arg[],
    spreads: unknown[][],
  ): { sql: string; bindings: unknown[] }[] {
    const isSpread = args.map((a) => a.kind === "field");
    const fixedCount = isSpread.filter((s) => !s).length;
    const spreadCount = isSpread.length - fixedCount;
    const size = Math.max(1, Math.floor((MAX_BOUND_PARAMETERS - fixedCount) / (spreadCount || 1)));

    const perArgChunks = spreads.map((values, i) => (isSpread[i] ? chunk(values, size) : [values]));
    return cartesian(perArgChunks).map((chunks) => ({
      sql: expandPlaceholders(
        sql,
        chunks.map((c) => c.length),
      ),
      bindings: chunks.flat(),
    }));
  }

  function chunk<T>(values: T[], size: number): T[][] {
    const out: T[][] = [];
    for (let i = 0; i < values.length; i += size) out.push(values.slice(i, i + size));
    return out;
  }

  function cartesian<T>(lists: T[][]): T[][] {
    return lists.reduce<T[][]>(
      (acc, list) => acc.flatMap((prefix) => list.map((item) => [...prefix, item])),
      [[]],
    );
  }

  /** Rewrite each `?N` placeholder into the N-th argument's anonymous `?` expansion. */
  function expandPlaceholders(sql: string, counts: number[]): string {
    const [head, ...rest] = sql.split("?");
    return rest.reduce((out, part) => {
      const digits = part.match(/^\d+/)?.[0];
      if (!digits) throw new Error(`unnumbered placeholder in plan SQL: ${sql}`);
      const expansion = Array(counts[Number(digits) - 1])
        .fill("?")
        .join(", ");
      return out + expansion + part.slice(digits.length);
    }, head);
  }
}

namespace save {
  /** A step's output, deferred to the sequential sink: body attachments or a synthesize. */
  type Sunk = { kind: "attach"; entries: [PathSegment[], unknown][] } | { kind: "synthesize" };

  /** The storage a step targets, for error typing; Synthesize steps touch none. */
  function database(step: SaveStep): Database | null {
    const q = step.query;
    if ("SqlBatch" in q) return q.SqlBatch.database;
    if ("KeyWrite" in q) return q.KeyWrite.database;
    return null;
  }

  export class Execution {
    /** The hydrated body, mutated stage by stage; `Result` args read from it. */
    private body: any = null;

    constructor(private storage: StorageResolver) {}

    async run(plan: SavePlan): Promise<CloesceResult<any>> {
      const errors: CloesceErrorKind[] = [];
      for (const stage of plan.stages) {
        const settled = await Promise.allSettled(stage.steps.map((s) => this.runStep(s)));
        settled.forEach((res, i) => {
          const step = stage.steps[i];
          try {
            if (res.status === "fulfilled") this.sink(step, res.value);
            else throw res.reason;
          } catch (e) {
            errors.push(stepError(database(step), e));
          }
        });
      }
      return sinkResult(this.body, errors);
    }

    private async runStep(step: SaveStep): Promise<Sunk> {
      const q = step.query;
      if ("SqlBatch" in q) return { kind: "attach", entries: await this.runBatch(q.SqlBatch) };
      if ("KeyWrite" in q) {
        return { kind: "attach", entries: [[step.result, await this.runKeyWrite(q.KeyWrite)]] };
      }
      return { kind: "synthesize" };
    }

    /** Run one batch transactionally, returning each Hydrate read-back keyed by its path. */
    private async runBatch(q: {
      database: Database;
      statements: SqlStatement[];
      shard: [string, SaveArg][];
    }): Promise<[PathSegment[], unknown][]> {
      const tags = q.shard.map(([field, a]) => [field, this.resolve(a)] as const);
      const store = this.storage.sql(
        q.database,
        tags.map(([, v]) => v),
      );

      const results = await store.batch(
        q.statements.map((s) => {
          const spec = "Write" in s ? s.Write : s.Hydrate;
          return { sql: spec.sql, bindings: spec.arguments.map((a) => this.resolve(a)) };
        }),
      );

      return q.statements.flatMap((s, i): [PathSegment[], unknown][] => {
        if (!("Hydrate" in s)) return [];
        const row = results[i]?.[0];
        if (!row) {
          throw new Error(`hydrate at ${pathText(s.Hydrate.result)} returned no row`);
        }
        // Stamp shard fields so the body carries the row's full routing identity.
        return [[s.Hydrate.result, { ...row, ...Object.fromEntries(tags) }]];
      });
    }

    /** Write one key, returning the stored value for attachment at the step's result. */
    private async runKeyWrite(q: {
      database: Database;
      segments: TemplateSegment<SaveArg>[];
      value: unknown;
      metadata: unknown | null;
      shard: [string, SaveArg][];
    }): Promise<unknown> {
      const shard = q.shard.map(([, a]) => this.resolve(a));
      const key = renderKey(
        q.segments,
        templateArgs(q.segments).map((a) => this.resolve(a)),
      );
      await this.storage.key(q.database, shard).put(key, q.value, q.metadata ?? undefined);
      return q.value;
    }

    /** Resolve a statement argument: a payload literal or a hydrated-body reference. */
    private resolve(arg: SaveArg): unknown {
      if ("Payload" in arg) return arg.Payload;
      const value = valueAt(this.body, arg.Result);
      if (value === undefined) {
        throw new Error(`missing hydrated value at ${pathText(arg.Result)}`);
      }
      return value;
    }

    private sink(step: SaveStep, out: Sunk): void {
      if (out.kind === "attach") {
        for (const [path, value] of out.entries) this.body = attach(this.body, path, value);
        return;
      }
      const q = step.query;
      if ("Synthesize" in q) this.synthesize(step.result, q.Synthesize);
    }

    /**
     * `create` materializes a fresh value at the path; otherwise fields merge onto the
     * object already there (an absent slot is left untouched). Field resolution runs in
     * the sink so it sees the stage's earlier attachments.
     */
    private synthesize(
      result: PathSegment[],
      q: { fields: [string, SaveArg][]; create: boolean; cardinality: "One" | "Many" },
    ): void {
      const buildFields = () =>
        Object.fromEntries(q.fields.map(([field, a]) => [field, this.resolve(a)]));

      if (q.create) {
        const value =
          q.cardinality === "One" ? buildFields() : q.fields.length === 0 ? [] : [buildFields()];
        this.body = attach(this.body, result, value);
        return;
      }

      const target = valueAt(this.body, result);
      if (target !== null && typeof target === "object" && !Array.isArray(target)) {
        Object.assign(target, buildFields());
      }
    }
  }

  function pathText(path: PathSegment[]): string {
    return path.map((seg) => ("Field" in seg ? seg.Field : `[${seg.Index}]`)).join(".") || "(root)";
  }

  function valueAt(body: unknown, path: PathSegment[]): unknown {
    return path.reduce<any>(
      (cur, seg) => cur?.["Field" in seg ? seg.Field : seg.Index],
      body ?? undefined,
    );
  }

  /**
   * Place `value` at an exact path, creating intermediate objects/arrays on demand.
   * An empty path merges onto an existing root object (a child hydrated earlier may
   * already sit there) rather than replacing it. Returns the (possibly new) root.
   */
  function attach(root: any, path: PathSegment[], value: unknown): any {
    if (path.length === 0) {
      if (isObject(root) && isObject(value)) {
        return Object.assign(root, value);
      }
      return value;
    }

    if (!isContainer(root, path[0])) root = emptyFor(path[0]);
    let cur = root;
    for (let i = 0; i < path.length - 1; i++) {
      cur = descend(cur, path[i], path[i + 1]);
    }
    place(cur, path[path.length - 1], value);
    return root;
  }

  /** Step into (creating if needed) the child at `seg`; `next` decides its container kind. */
  function descend(cur: any, seg: PathSegment, next: PathSegment): any {
    if ("Field" in seg) {
      if (!isContainer(cur[seg.Field], next)) cur[seg.Field] = emptyFor(next);
      return cur[seg.Field];
    }
    while (cur.length <= seg.Index) cur.push(null);
    if (!isContainer(cur[seg.Index], next)) cur[seg.Index] = emptyFor(next);
    return cur[seg.Index];
  }

  function place(cur: any, seg: PathSegment, value: unknown): void {
    if ("Field" in seg) {
      cur[seg.Field] = value;
      return;
    }
    while (cur.length <= seg.Index) cur.push(null);
    cur[seg.Index] = value;
  }

  function isObject(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
  }

  /** Whether `value` is the container kind (object/array) that `seg` indexes into. */
  function isContainer(value: unknown, seg: PathSegment): boolean {
    return "Field" in seg ? isObject(value) : Array.isArray(value);
  }

  function emptyFor(seg: PathSegment): any {
    return "Field" in seg ? {} : [];
  }
}
