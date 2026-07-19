import { interpolate, sinkResult, stepError, templateArgs, StorageResolver } from "./index.js";
import { CloesceErrorKind, CloesceResult, InternalError } from "../../common.js";
import {
  Database,
  JoinKeys,
  Mapping,
  SelectArg,
  SelectPlan,
  SelectStep,
  TemplateSegment,
} from "./plan.js";
import { KValue } from "../../ui/backend.js";

/**
 * SQLite's bound-parameter budget per statement.
 *
 * TODO: This is a good place to optimize in the future
 * (e.g., insert into a temp table rather than use bound parameters)
 */
export const MAX_BOUND_PARAMETERS = 100;

interface Attachment {
  /** Back reference to the parent row */
  parent: number;

  /** A hydrated value */
  value: any;
}

/** The hydrated output of one step. */
interface Table {
  attachments: Attachment[];

  /** Whether the table contains multiple rows. */
  many: boolean;
}

/** A {@link SelectArg} split into its owner table path and field. */
type Arg = { kind: "param"; name: string } | { kind: "field"; owner: string[]; field: string };

type Fetched =
  /** A SQL query result. */
  | { kind: "rows"; rows: Record<string, unknown>[] }

  /** Key reads by their resolved argument tuple (JSON-encoded). */
  | { kind: "keys"; values: Map<string, unknown> }

  /** No data was produced. */
  | { kind: "none" };

export async function execute(
  plan: SelectPlan,
  storage: StorageResolver,
  params: Record<string, unknown>,
  seed?: Record<string, unknown>[],
): Promise<CloesceResult<any>> {
  const { seeded, body } = SeedFactory.seed(plan, params, seed ?? []);
  const exec = new Executor(storage, body);
  const sink = new ResultAssembler(body);

  const errors = [] as CloesceErrorKind[];
  for (const stage of plan.stages) {
    const pending = stage.steps.filter((s) => !seeded.has(JSON.stringify(s.result)));
    const settled = await Promise.allSettled(pending.map((s) => exec.fetch(s)));

    for (const [i, res] of settled.entries()) {
      const step = pending[i];
      try {
        if (res.status === "fulfilled") {
          sink.attach(step, res.value);
        } else {
          throw res.reason;
        }
      } catch (e) {
        errors.push(stepError(database(step), e));
        sink.attachBlank(step);
      }
    }
  }

  return sinkResult(sink.assemble(), errors);
}

/** An interface for the working set of all fetched/seeded data */
class WorkingResultBody {
  public tables: Map<string, Table> = new Map();
  constructor(private params: Record<string, unknown>) {}

  /** Get a table by its result path. */
  get(path: string[]): Table | undefined {
    return this.tables.get(JSON.stringify(path));
  }

  /** Set a table by its result path. */
  set(path: string[], table: Table): void {
    this.tables.set(JSON.stringify(path), table);
  }

  /**
   * Resolve an argument for the object at (`path`, `idx`), climbing the parent
   * back-refs up to the argument's owner table.
   *
   * @returns `undefined` when a value is absent.
   */
  resolve(path: string[], idx: number, a: Arg): unknown | undefined {
    return this.resolver(path, a)(idx);
  }

  /**
   * An argument resolver for objects of the table at `path`.
   *
   * Parent climb tables are looked up once, so resolving each object only
   * walks back-refs.
   */
  resolver(path: string[], a: Arg): (idx: number) => unknown | undefined {
    if (a.kind === "param") {
      const value = this.param(a.name);
      return () => value;
    }

    const climb: (Table | undefined)[] = [];
    let cur = path;
    while (cur.length > a.owner.length) {
      climb.push(this.get(cur));
      cur = cur.slice(0, -1);
    }
    const owner = this.get(cur);

    return (idx) => {
      for (const table of climb) {
        idx = table?.attachments[idx]?.parent ?? 0;
      }
      return owner?.attachments[idx]?.value?.[a.field];
    };
  }

  /** Get a parameter by its name; a plan may only demand params the caller supplied. */
  param(name: string): unknown {
    if (!(name in this.params)) {
      throw new Error(`missing parameter "${name}"`);
    }
    return this.params[name];
  }

  /**
   * The distinct value tuples of `args`.
   *
   * - One tuple per hydrated object of the deepest owner table any argument
   *   references (a single tuple when every arg is a param).
   * - An arg owned by an ancestor of that table resolves by climbing the parent
   *   back-refs.
   * - Tuples with a missing value are skipped.
   */
  tuples(args: Arg[], at: string[]): unknown[][] {
    const owners = args.flatMap((a) => (a.kind === "field" ? [a.owner] : []));
    if (owners.length === 0) {
      return [args.map((a) => this.resolve([], 0, a))];
    }

    const deepest = owners.reduce((a, b) => (b.length > a.length ? b : a));
    const table = this.get(deepest);
    if (table === undefined) {
      throw new InternalError(
        `select step at ${JSON.stringify(at)} reads table ${JSON.stringify(deepest)} ` +
          `before an earlier stage produced it`,
      );
    }

    const resolvers = args.map((a) => this.resolver(deepest, a));
    const seen = new Set<string>();
    const out: unknown[][] = [];
    for (let idx = 0; idx < table.attachments.length; idx++) {
      const tuple = resolvers.map((r) => r(idx));

      if (tuple.some((v) => v === undefined)) {
        continue;
      }

      const key = JSON.stringify(tuple);
      if (!seen.has(key)) {
        seen.add(key);
        out.push(tuple);
      }
    }

    return out;
  }
}

class Executor {
  constructor(
    private storage: StorageResolver,
    private body: WorkingResultBody,
  ) {}

  async fetch(step: SelectStep): Promise<Fetched> {
    const q = step.query;
    if ("Sql" in q) {
      return { kind: "rows", rows: await this.fetchSql(step, q.Sql) };
    }

    if ("Key" in q) {
      return { kind: "keys", values: await this.fetchKeys(step, q.Key) };
    }

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

    // The full set of distinct values that will be bound to the SQL query.
    const spreads = args.map((a) => this.body.tuples([a], step.result).map(([v]) => v));

    if (spreads.some((s) => s.length === 0)) {
      // Spread over zero values selects nothing.
      return [];
    }
    const statements = chunkStatements(q.sql, args, spreads);

    const shardArgs = (q.shard ?? []).map(([, a]) => arg(a));
    const shardValues = this.body.tuples(shardArgs, step.result);

    const constantFields = (q.route_fields ?? []).flatMap(([field, raw]) => {
      const a = arg(raw);
      if (a.kind === "field") {
        // Deferred to assembly
        return [];
      }

      // Param route fields are constant across shards.
      return [[field, this.body.param(a.name)] as const];
    });

    const perShard = await Promise.all(
      shardValues.map(async (tuple) => {
        const store = this.storage.sql(q.database, tuple);
        const results =
          statements.length === 1
            ? [await store.query(statements[0].sql, statements[0].bindings)]
            : await store.batch(statements);

        // Stamp shard fields so joins and deeper shards can read them off the row.
        const stamps = [
          ...q.shard.map(([field], i) => [field, tuple[i]] as const),
          ...constantFields,
        ];
        return results.flat().map((row) => {
          for (const [field, value] of stamps) {
            row[field] = value;
          }
          return row;
        });
      }),
    );

    return perShard.flat();
  }

  /**
   * Read from a key store. Makes one fetch per distinct parent row, concurrently.
   */
  private async fetchKeys(
    step: SelectStep,
    q: {
      database: Database;
      segments: TemplateSegment<SelectArg>[];
      shard: [string, SelectArg][];
    },
  ): Promise<Map<string, unknown>> {
    const args = keyArgs(q.shard, q.segments);
    const tuples = this.body.tuples(args, step.result);

    const fetches = tuples.map(async (tuple) => {
      // Tuple contains the shard values first, then the template values.
      const shardValues = tuple.slice(0, q.shard.length);
      const keyValues = tuple.slice(q.shard.length);

      const key = interpolate(q.segments, keyValues);
      const raw = await this.storage.key(q.database, shardValues).get(key);

      let wrapped: unknown = raw ?? null;
      if (q.database.kind === "Kv") {
        // Coerce into a KValue
        const inner = (raw ?? {}) as { value?: unknown; metadata?: unknown };
        wrapped = new KValue(inner.value ?? null, inner.metadata ?? null);
      }

      return [JSON.stringify(tuple), wrapped] as const;
    });

    const entries = await Promise.all(fetches);
    return new Map(entries);
  }
}

/** Sinks fetched step outputs into the working body and folds it into the final result. */
class ResultAssembler {
  constructor(private body: WorkingResultBody) {}

  /** Fold every table into its parent, deepest first, producing the final body. */
  assemble(): unknown {
    const tables = [...this.body.tables.entries()]
      .map(([key, table]) => ({ path: JSON.parse(key) as string[], table }))
      .sort((a, b) => b.path.length - a.path.length);

    for (const { path, table: child } of tables) {
      if (path.length === 0) {
        break;
      }

      const field = path[path.length - 1];
      const parent = this.body.get(path.slice(0, -1));
      if (!parent) {
        continue;
      }
      if (child.many) {
        for (const a of parent.attachments) {
          a.value[field] = [];
        }
      }
      for (const att of child.attachments) {
        const slot = parent.attachments[att.parent];
        if (!slot) {
          continue;
        }
        if (child.many) {
          slot.value[field].push(att.value);
        } else {
          slot.value[field] = att.value;
        }
      }
    }

    const root = this.body.get([]);
    if (!root) {
      return null;
    }
    const values = root.attachments.map((a) => a.value);
    return root.many ? values : (values[0] ?? null);
  }

  /** Attach the fetched data to the step's result. */
  attach(step: SelectStep, fetched: Fetched): void {
    const q = step.query;
    if ("Sql" in q && fetched.kind === "rows") {
      this.body.set(step.result, this.rowTable(step.result, q.Sql, fetched.rows));
    } else if ("Key" in q && fetched.kind === "keys") {
      const parentPath = step.result.slice(0, -1);
      const resolvers = keyArgs(q.Key.shard, q.Key.segments).map((a) =>
        this.body.resolver(parentPath, a),
      );
      this.body.set(
        step.result,
        this.singletons(step.result, (idx) => {
          const tuple = resolvers.map((r) => r(idx));
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
  attachBlank(step: SelectStep): void {
    if (this.body.get(step.result)) {
      return;
    }
    const q = step.query;
    const many = "Sql" in q && q.Sql.mapping.cardinality === "Many";
    this.body.set(step.result, { attachments: [], many });
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
    const parent = this.body.get(parentPath);
    if (!parent) {
      return { attachments: [], many };
    }

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

    const routeResolvers = (q.route_fields ?? []).flatMap(([field, raw]) => {
      const a = arg(raw);
      if (a.kind !== "field") {
        return [];
      }
      return [[field, this.body.resolver(parentPath, a)] as const];
    });

    const served = new Set<number>();
    const attachments: Attachment[] = [];
    for (const row of rows) {
      for (const p of index.get(joinKey(row, (j) => j.child_key)) ?? []) {
        if (!many) {
          if (served.has(p)) {
            continue;
          }
          served.add(p);
        }
        // Clone: a row may attach under several parents, each hydrated independently.
        const value: Record<string, unknown> = { ...row };
        for (const [field, resolve] of routeResolvers) {
          value[field] = resolve(p);
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

    const existing = this.body.get(path);
    if (existing) {
      for (const att of existing.attachments) {
        for (const [field, a] of fields) {
          att.value[field] = this.body.resolve(parentPath, att.parent, a);
        }
      }
      return;
    }

    // A Many synthesize is a singleton array, folded as a single value.
    // At the root every field is param-sourced by construction.
    this.body.set(
      path,
      this.singletons(path, (idx) => {
        const object = Object.fromEntries(
          fields.map(([field, a]) => [field, this.body.resolve(parentPath, idx, a)]),
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
    const parent = this.body.get(path.slice(0, -1));
    const attachments = (parent?.attachments ?? []).map((_, i) => ({
      parent: i,
      value: build(i),
    }));
    return { attachments, many: false };
  }
}

class SeedFactory {
  private constructor(
    private seeded: Set<string>,
    private body: WorkingResultBody,
  ) {}

  /**
   * @remarks
   *  - A step is seeded when its parent path is already seeded and its field is
   *   present (`!== undefined`) on at least one parent attachment.
   * - Parents lacking the field contribute no attachments, so they degrade to
   *   empty. Missing data is never an error.
   * - Each seeded step also gets its derived (shard, route, synthesized) fields
   *   stamped, mirroring a live fetch so unseeded descendants still route and join.
   * - Seeds are **mutated in place**.
   *
   * @param plan The select plan to seed for.
   *  Result paths in the plan are stored in the seeded set.
   * @param params The parameters to bind to the plan's `Param` arguments.
   * @param seed The seed data to use for populating the tables.
   *  Data within the seed is placed directly into the tables result.
   *
   * @returns the set of seeded paths and the hydrated body.
   */
  static seed(
    plan: SelectPlan,
    params: Record<string, unknown>,
    seed: Record<string, unknown>[],
  ): { seeded: Set<string>; body: WorkingResultBody } {
    const factory = new SeedFactory(new Set<string>(), new WorkingResultBody(params));
    return factory.run(plan, seed);
  }

  private run(
    plan: SelectPlan,
    seed: Record<string, unknown>[],
  ): { seeded: Set<string>; body: WorkingResultBody } {
    if (seed.length === 0) {
      return { seeded: this.seeded, body: this.body };
    }

    this.body.set([], {
      attachments: seed.map((value) => ({ parent: 0, value })),
      many: true,
    });
    this.seeded.add(JSON.stringify([]));

    for (const stage of plan.stages) {
      for (const step of stage.steps) {
        const path = step.result;
        const parentSeeded = this.seeded.has(JSON.stringify(path.slice(0, -1)));
        const seededHere = this.seeded.has(JSON.stringify(path));

        const seeded = parentSeeded && !seededHere && this.seedChild(step);

        if (path.length === 0 || seeded) {
          this.stampDerived(step);
        }
      }
    }

    return { seeded: this.seeded, body: this.body };
  }

  /**
   * Build a child step's table from the field the seed supplied on its parent.
   *
   * - Returns `false` when the field is absent on every parent.
   * - `Many` fields flatten their array elements into attachments (`[]` seeds empty);
   *   `One`/`Key` fields attach one non-null value. A `null`, or an absent parent,
   *   contributes nothing.
   */
  private seedChild(step: SelectStep): boolean {
    const path = step.result;
    const field = path[path.length - 1];
    const parent = this.body.get(path.slice(0, -1))!;
    if (parent.attachments.every((a) => a.value[field] === undefined)) {
      return false;
    }

    const q = step.query;
    let many = false;
    if ("Sql" in q) {
      many = q.Sql.mapping.cardinality === "Many";
    } else if ("Synthesize" in q) {
      many = q.Synthesize.cardinality === "Many";
    }

    const attachments = [] as Attachment[];
    for (const [idx, a] of parent.attachments.entries()) {
      const value = a.value[field];
      if (value === undefined || value === null) {
        continue;
      }

      if (!many) {
        attachments.push({ parent: idx, value });
        continue;
      }

      if (!Array.isArray(value)) {
        // Fail silently. Subsequent steps will no-op.
        continue;
      }

      for (const v of value) {
        attachments.push({ parent: idx, value: v });
      }
    }

    this.body.set(path, { attachments, many });
    this.seeded.add(JSON.stringify(path));
    return true;
  }

  /**
   * Fill a seeded step's derived fields where the seed left them undefined, so the
   * seed matches what a live fetch would have produced.
   */
  private stampDerived(step: SelectStep): void {
    const q = step.query;

    let fields = [] as [string, SelectArg][];
    if ("Sql" in q) {
      fields = [...(q.Sql.shard ?? []), ...(q.Sql.route_fields ?? [])];
    } else if ("Synthesize" in q) {
      fields = q.Synthesize.fields;
    }

    if (fields.length === 0) {
      return;
    }

    const table = this.body.get(step.result)!;
    for (const [field, raw] of fields) {
      let resolve: (idx: number) => unknown;
      try {
        resolve = this.body.resolver(step.result, arg(raw));
      } catch {
        // A seed needn't supply what a live fetch would have; leave the field absent.
        continue;
      }

      for (const [idx, a] of table.attachments.entries()) {
        if (a.value[field] !== undefined) {
          continue;
        }

        const value = resolve(idx);
        if (value !== undefined) {
          a.value[field] = value;
        }
      }
    }
  }
}

function database(step: SelectStep): Database | null {
  const q = step.query;
  if ("Sql" in q) {
    return q.Sql.database;
  }
  if ("Key" in q) {
    return q.Key.database;
  }
  return null;
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
 *
 * For example, `SELECT * FROM t WHERE a IN (?1) AND b IN (?2)`
 * with `?1` bound to `[1, 2...100]` and `?2` bound to `[101, 102, 103]` produces:
 * ```
 * { sql: "... a IN (?, ?, ... 50) AND b IN (?, ?, ?)", bindings: [1..50, 101, 102, 103] }
 * { sql: "... a IN (?, ?, ... 50) AND b IN (?, ?, ?)", bindings: [51..100, 101, 102, 103] }
 * ```
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

  // Each chunk is a cross prod of the per-arg chunks.
  return cartesian(perArgChunks).map((chunks) => ({
    sql: expandPlaceholders(
      sql,
      chunks.map((c) => c.length),
    ),
    bindings: chunks.flat(),
  }));

  function cartesian<T>(lists: T[][]): T[][] {
    return lists.reduce<T[][]>(
      (acc, list) => acc.flatMap((prefix) => list.map((item) => [...prefix, item])),
      [[]],
    );
  }

  function chunk<T>(values: T[], size: number): T[][] {
    const out: T[][] = [];
    for (let i = 0; i < values.length; i += size) {
      out.push(values.slice(i, i + size));
    }
    return out;
  }

  /** Rewrite each `?N` placeholder into the N-th argument's anonymous `?` expansion. */
  function expandPlaceholders(sql: string, counts: number[]): string {
    const [head, ...rest] = sql.split("?");
    return rest.reduce((out, part) => {
      const digits = part.match(/^\d+/)?.[0];
      if (!digits) {
        throw new InternalError(`unnumbered placeholder in plan SQL: ${sql}`);
      }
      const expansion = Array(counts[Number(digits) - 1])
        .fill("?")
        .join(", ");
      return out + expansion + part.slice(digits.length);
    }, head);
  }
}

function arg(raw: SelectArg): Arg {
  if ("Param" in raw) {
    return { kind: "param", name: raw.Param };
  }
  const path = raw.Result;
  return {
    kind: "field",
    owner: path.slice(0, -1),
    field: path[path.length - 1],
  };
}
