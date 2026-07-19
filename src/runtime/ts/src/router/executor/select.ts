import {
  chunk,
  interpolate,
  sinkResult,
  stepError,
  templateArgs,
  KeyStore,
  StorageResolver,
} from "./index.js";
import { CloesceErrorKind, CloesceResult, InternalError } from "../../common.js";
import {
  Database,
  JoinKeys,
  Mapping,
  SelectArg,
  SelectPlan,
  SelectStep,
  SqlArgument,
  SqlSegment,
  TableDef,
  TemplateSegment,
} from "./plan.js";
import { KValue } from "../../ui/backend.js";

/**
 *
 * D1's documented limit is "Maximum bound parameters per query: 100", per
 * https://developers.cloudflare.com/d1/platform/limits/, and applies per statement within
 * batches.
 *
 * TODO: This is a good place to optimize in the future
 * (e.g., insert into a temp table rather than use bound parameters)
 */
export const MAX_BOUND_PARAMETERS = 100;

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
    const pending = stage.steps.filter((s) => !seeded.has(s.table));
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

/** A {@link SelectArg} split into its owner table id and field. */
type Arg = { kind: "param"; name: string } | { kind: "field"; table: number; field: string };

type Fetched =
  /** A SQL query result. */
  | { kind: "rows"; rows: Record<string, unknown>[] }

  /** Key reads by their resolved argument tuple (JSON-encoded). */
  | { kind: "keys"; values: Map<string, unknown> }

  /** No data was produced. */
  | { kind: "none" };

/** An interface for the working set of all fetched/seeded data */
class WorkingResultBody {
  /** The hydrated tables of every completed step, indexed by table id. */
  public tables: (Table | undefined)[] = [];

  constructor(
    public defs: TableDef[],
    private params: Record<string, unknown>,
  ) {}

  /** Get a table by its id. */
  get(table: number): Table | undefined {
    return this.tables[table];
  }

  /** Set a table by its id. */
  set(table: number, value: Table): void {
    this.tables[table] = value;
  }

  /** The parent table of `table`, or `undefined` for the root. */
  parentTable(table: number): number | undefined {
    return this.defs[table]?.parent?.table;
  }

  /** Whether `ancestor` is `node` or one of its ancestors. */
  isAncestor(ancestor: number, node: number): boolean {
    let cur: number | undefined = node;
    while (cur !== undefined) {
      if (cur === ancestor) {
        return true;
      }
      cur = this.parentTable(cur);
    }
    return false;
  }

  /**
   * Resolve an argument for the object at (`table`, `idx`), climbing the parent
   * back-refs up to the argument's owner table.
   *
   * @returns `undefined` when a value is absent.
   */
  resolve(table: number, idx: number, a: Arg): unknown | undefined {
    return this.resolver(table, a)(idx);
  }

  /**
   * An argument resolver for objects of the table at `table`.
   *
   * Parent climb tables are looked up once, so resolving each object only
   * walks back-refs.
   */
  resolver(table: number, a: Arg): (idx: number) => unknown | undefined {
    if (a.kind === "param") {
      const value = this.param(a.name);
      return () => value;
    }

    const climb: (Table | undefined)[] = [];
    let cur: number | undefined = table;
    while (cur !== undefined && cur !== a.table) {
      climb.push(this.get(cur));
      cur = this.parentTable(cur);
    }
    const owner = cur === a.table ? this.get(a.table) : undefined;

    return (idx) => {
      for (const t of climb) {
        idx = t?.attachments[idx]?.parent ?? 0;
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
  tuples(args: Arg[], at: number): unknown[][] {
    const tables = args.flatMap((a) => (a.kind === "field" ? [a.table] : []));
    if (tables.length === 0) {
      return [args.map((a) => this.resolve(0, 0, a))];
    }

    // The referenced tables all lie on one ancestor line, so the deepest is the one
    // with the largest id (a child's id exceeds its parent's).
    const deepest = Math.max(...tables);
    if (!tables.every((t) => this.isAncestor(t, deepest))) {
      throw new InternalError(
        `select step for table ${at} reads key/shard arguments spanning unrelated source tables`,
      );
    }

    const table = this.get(deepest);
    if (table === undefined) {
      throw new InternalError(
        `select step for table ${at} reads table ${deepest} before an earlier stage produced it`,
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
      sql: SqlSegment[];
      arguments: SqlArgument[];
      shard: [string, SelectArg][];
      route_fields: [string, SelectArg][];
    },
  ): Promise<Record<string, unknown>[]> {
    const shardArgs = q.shard.map(([, a]) => arg(a));
    const spreads = q.arguments.map((sa) => sa.spread);

    // One deduped tuple per distinct (shard values..., argument values...) combination,
    // resolved together in a single owner-table pass. Params bind their constant value; a
    // tuple with any missing value is dropped, so a spread over zero parent values selects
    // nothing.
    const combined = this.body.tuples(
      [...shardArgs, ...q.arguments.map((sa) => arg(sa.value))],
      step.table,
    );

    const shardLen = q.shard.length;

    const constantFields = q.route_fields.flatMap(([field, raw]) => {
      const a = arg(raw);
      // Field route fields are deferred to assembly; params are constant across shards.
      return a.kind === "field" ? [] : [[field, this.body.param(a.name)] as const];
    });

    // Group by shard prefix; each shard resolves and chunks its own arguments in isolation.
    const groups = new Map<string, unknown[][]>();
    for (const tuple of combined) {
      const slot = JSON.stringify(tuple.slice(0, shardLen));
      (groups.get(slot) ?? groups.set(slot, []).get(slot)!).push(tuple);
    }

    const perGroup = await Promise.all(
      [...groups.values()].map(async (tuples) => {
        const shardTuple = tuples[0].slice(0, shardLen);

        // Within this shard each argument dedupes its own column: a spread keeps every value,
        // a non-spread argument must resolve to exactly one.
        const values = q.arguments.map((_, i) => {
          const column = distinct(tuples.map((t) => t[shardLen + i]));
          if (!spreads[i] && column.length > 1) {
            throw new InternalError(
              `select step for table ${step.table} binds a non-spread argument resolving to ${column.length} distinct values`,
            );
          }
          return column;
        });

        if (values.some((v) => v.length === 0)) {
          // A bind over zero values selects nothing in this shard.
          return [];
        }

        const statements = chunkStatements(q.sql, spreads, values);
        const store = this.storage.sql(q.database, shardTuple);
        const results =
          statements.length === 1
            ? [await store.query(statements[0].sql, statements[0].bindings)]
            : await store.batch(statements);

        // Stamp shard fields so joins and deeper shards can read them off the row.
        const stamps = [
          ...q.shard.map(([field], i) => [field, shardTuple[i]] as const),
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

    return perGroup.flat();
  }

  /**
   * Read from a key store. Groups keys by their shard store so a store that supports
   * bulk reads fetches them in one {@link KeyStore.getMany} call; the rest fall back to
   * per-key {@link KeyStore.get}, concurrently.
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
    const tuples = this.body.tuples(args, step.table);

    const wrap = (raw: unknown): unknown => {
      if (q.database.kind !== "Kv") {
        return raw ?? null;
      }
      // Coerce into a KValue.
      const inner = (raw ?? {}) as { value?: unknown; metadata?: unknown };
      return new KValue(inner.value ?? null, inner.metadata ?? null);
    };

    // Group tuples by their shard store (tuple = [...shard values, ...template values]).
    const groups = new Map<
      string,
      { store: KeyStore; items: { tuple: unknown[]; key: string }[] }
    >();
    for (const tuple of tuples) {
      const shardValues = tuple.slice(0, q.shard.length);
      const key = interpolate(q.segments, tuple.slice(q.shard.length));
      const slot = JSON.stringify(shardValues);
      const group =
        groups.get(slot) ??
        groups
          .set(slot, { store: this.storage.key(q.database, shardValues), items: [] })
          .get(slot)!;
      group.items.push({ tuple, key });
    }

    const entries = await Promise.all(
      [...groups.values()].map(async ({ store, items }) => {
        if (store.getMany) {
          const found = await store.getMany(items.map((it) => it.key));
          return items.map((it) => [JSON.stringify(it.tuple), wrap(found.get(it.key))] as const);
        }
        return Promise.all(
          items.map(
            async (it) => [JSON.stringify(it.tuple), wrap(await store.get(it.key))] as const,
          ),
        );
      }),
    );

    return new Map(entries.flat());
  }
}

/** Sinks fetched step outputs into the working body and folds it into the final result. */
class ResultAssembler {
  constructor(private body: WorkingResultBody) {}

  /** Fold every table into its parent, deepest first, producing the final body. */
  assemble(): unknown {
    // A child's id always exceeds its parent's (a plan invariant), so folding in
    // descending id order always hydrates a child before its parent.
    const ids = this.body.defs.map((_, id) => id).sort((a, b) => b - a);

    for (const id of ids) {
      const parent = this.body.defs[id].parent;
      if (!parent) {
        continue;
      }

      const child = this.body.get(id);
      if (!child) {
        continue;
      }
      const slot = this.body.get(parent.table);
      if (!slot) {
        continue;
      }

      const field = parent.field;
      if (child.many) {
        for (const a of slot.attachments) {
          a.value[field] = [];
        }
      }
      for (const att of child.attachments) {
        const target = slot.attachments[att.parent];
        if (!target) {
          continue;
        }
        if (child.many) {
          target.value[field].push(att.value);
        } else {
          target.value[field] = att.value;
        }
      }
    }

    const root = this.body.get(0);
    if (!root) {
      return null;
    }
    const values = root.attachments.map((a) => a.value);
    return root.many ? values : (values[0] ?? null);
  }

  /** Attach the fetched data to the step's table. */
  attach(step: SelectStep, fetched: Fetched): void {
    const q = step.query;
    const table = step.table;
    if ("Sql" in q && fetched.kind === "rows") {
      this.body.set(table, this.rowTable(table, q.Sql, fetched.rows));
    } else if ("Key" in q && fetched.kind === "keys") {
      const at = this.resolveTable(table);
      const resolvers = keyArgs(q.Key.shard, q.Key.segments).map((a) => this.body.resolver(at, a));
      this.body.set(
        table,
        this.singletons(table, (idx) => {
          const tuple = resolvers.map((r) => r(idx));
          return fetched.values.get(JSON.stringify(tuple)) ?? null;
        }),
      );
    } else if ("Synthesize" in q) {
      this.synthesize(table, q.Synthesize);
    }
  }

  /**
   * Sink a failed step as an empty table (unless a sibling already produced one for the
   * table), so steps of later stages that read it degrade to empty instead of failing.
   */
  attachBlank(step: SelectStep): void {
    if (this.body.get(step.table)) {
      return;
    }
    const q = step.query;
    const many = "Sql" in q && q.Sql.mapping.cardinality === "Many";
    this.body.set(step.table, { attachments: [], many });
  }

  /** Tie each fetched row to its parents via the mapping's join keys. */
  private rowTable(
    table: number,
    q: { mapping: Mapping; route_fields: [string, SelectArg][] },
    rows: Record<string, unknown>[],
  ): Table {
    const many = q.mapping.cardinality === "Many";
    const parent = this.body.defs[table].parent;
    if (!parent) {
      return { attachments: rows.map((value) => ({ parent: 0, value })), many };
    }

    const parents = this.body.get(parent.table);
    if (!parents) {
      return { attachments: [], many };
    }

    // Bucket parents by join key, then hand each row to its matching parents;
    // a One mapping serves each parent at most once.
    const match = bucketParents(parents.attachments, q.mapping.join);

    const routeResolvers = (q.route_fields ?? []).flatMap(([field, raw]) => {
      const a = arg(raw);
      if (a.kind !== "field") {
        return [];
      }
      return [[field, this.body.resolver(parent.table, a)] as const];
    });

    const served = new Set<number>();
    const cloned = new Set<Record<string, unknown>>();
    const attachments: Attachment[] = [];
    for (const row of rows) {
      for (const p of match(row)) {
        if (!many) {
          if (served.has(p)) {
            continue;
          }
          served.add(p);
        }
        // A row may attach under several parents, each hydrated independently: keep the
        // fetched object for its first parent and clone it for any further ones.
        const value: Record<string, unknown> = cloned.has(row) ? { ...row } : row;
        cloned.add(row);
        for (const [field, resolve] of routeResolvers) {
          value[field] = resolve(p);
        }
        attachments.push({ parent: p, value });
      }
    }
    return { attachments, many };
  }

  /**
   * Materialize or merge a synthesized object. When a table already exists (produced by an
   * earlier step), fields merge onto each of its values; otherwise a fresh singleton is
   * built per parent.
   */
  private synthesize(
    table: number,
    q: { fields: [string, SelectArg][]; cardinality: "One" | "Many" },
  ): void {
    const at = this.resolveTable(table);
    // Build each field's parent-climb resolver once, then resolve it per row.
    const resolvers = q.fields.map(
      ([field, raw]) => [field, this.body.resolver(at, arg(raw))] as const,
    );

    const existing = this.body.get(table);
    if (existing) {
      for (const att of existing.attachments) {
        for (const [field, resolve] of resolvers) {
          att.value[field] = resolve(att.parent);
        }
      }
      return;
    }

    // A Many synthesize is a singleton array, folded as a single value.
    // At the root every field is param-sourced by construction.
    this.body.set(
      table,
      this.singletons(table, (idx) => {
        const object = Object.fromEntries(
          resolvers.map(([field, resolve]) => [field, resolve(idx)]),
        );
        return q.cardinality === "One" ? object : [object];
      }),
    );
  }

  /** The table an object's per-parent fields resolve against: its parent, or itself at the root. */
  private resolveTable(table: number): number {
    return this.body.defs[table].parent?.table ?? table;
  }

  /** A table with exactly one value per parent row (one root value when `table` is the root). */
  private singletons(table: number, build: (parentIdx: number) => unknown): Table {
    const parent = this.body.defs[table].parent;
    if (!parent) {
      return { attachments: [{ parent: 0, value: build(0) }], many: false };
    }
    const parents = this.body.get(parent.table);
    const attachments = (parents?.attachments ?? []).map((_, i) => ({
      parent: i,
      value: build(i),
    }));
    return { attachments, many: false };
  }
}

class SeedFactory {
  private constructor(
    private seeded: Set<number>,
    private body: WorkingResultBody,
  ) {}

  /**
   * @remarks
   *  - A step is seeded when its parent table is already seeded and its field is
   *   present (`!== undefined`) on at least one parent attachment.
   * - Parents lacking the field contribute no attachments, so they degrade to
   *   empty. Missing data is never an error.
   * - Each seeded step also gets its derived (shard, route, synthesized) fields
   *   stamped, mirroring a live fetch so unseeded descendants still route and join.
   * - Seeds are **mutated in place**.
   *
   * @param plan The select plan to seed for.
   *  Table ids in the plan are stored in the seeded set.
   * @param params The parameters to bind to the plan's `Param` arguments.
   * @param seed The seed data to use for populating the tables.
   *  Data within the seed is placed directly into the tables result.
   *
   * @returns the set of seeded table ids and the hydrated body.
   */
  static seed(
    plan: SelectPlan,
    params: Record<string, unknown>,
    seed: Record<string, unknown>[],
  ): { seeded: Set<number>; body: WorkingResultBody } {
    const factory = new SeedFactory(new Set<number>(), new WorkingResultBody(plan.tables, params));
    return factory.run(plan, seed);
  }

  private run(
    plan: SelectPlan,
    seed: Record<string, unknown>[],
  ): { seeded: Set<number>; body: WorkingResultBody } {
    if (seed.length === 0) {
      return { seeded: this.seeded, body: this.body };
    }

    this.body.set(0, {
      attachments: seed.map((value) => ({ parent: 0, value })),
      many: true,
    });
    this.seeded.add(0);

    for (const stage of plan.stages) {
      for (const step of stage.steps) {
        const parent = this.body.defs[step.table].parent;
        const parentSeeded = parent === null || this.seeded.has(parent.table);
        const seededHere = this.seeded.has(step.table);

        const seeded = parentSeeded && !seededHere && this.seedChild(step);

        if (parent === null || seeded) {
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
    const parent = this.body.defs[step.table].parent!;
    const field = parent.field;
    const parents = this.body.get(parent.table)!;
    if (parents.attachments.every((a) => a.value[field] === undefined)) {
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
    for (const [idx, a] of parents.attachments.entries()) {
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

    this.body.set(step.table, { attachments, many });
    this.seeded.add(step.table);
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

    const table = this.body.get(step.table)!;
    for (const [field, raw] of fields) {
      let resolve: (idx: number) => unknown;
      try {
        resolve = this.body.resolver(step.table, arg(raw));
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
 * Render a SQL statement's segments into concrete `(sql, bindings)` chunks. Every chunk
 * stays within {@link MAX_BOUND_PARAMETERS}: fixed args are re-bound in each, and spread
 * values split the remaining budget evenly. Multiple spreads chunk as a cross product so
 * every value combination is covered exactly once.
 *
 * A {@link SqlSegment.Bind} renders as one `?` per bound value; a spread argument expands
 * to its chunk's `?, ?, ...` list.
 *
 * For example, `SELECT * FROM t WHERE a IN (Bind 0) AND b IN (Bind 1)`
 * with arg 0 spread over `[1, 2...100]` and arg 1 spread over `[101, 102, 103]` produces:
 * ```
 * { sql: "... a IN (?, ?, ... 50) AND b IN (?, ?, ?)", bindings: [1..50, 101, 102, 103] }
 * { sql: "... a IN (?, ?, ... 50) AND b IN (?, ?, ?)", bindings: [51..100, 101, 102, 103] }
 * ```
 */
function chunkStatements(
  sql: SqlSegment[],
  spreads: boolean[],
  values: unknown[][],
): { sql: string; bindings: unknown[] }[] {
  const fixedCount = spreads.filter((s) => !s).length;
  const spreadCount = spreads.length - fixedCount;
  const size = Math.max(1, Math.floor((MAX_BOUND_PARAMETERS - fixedCount) / (spreadCount || 1)));

  const perArgChunks = values.map((vals, i) => (spreads[i] ? chunk(vals, size) : [vals]));

  // Each chunk is a cross prod of the per-arg chunks.
  return cartesian(perArgChunks).map((chunks) => render(sql, chunks));

  /** Join segments into SQL, binding each `Bind` to its argument's chunk (in `?` order). */
  function render(
    segments: SqlSegment[],
    chunks: unknown[][],
  ): { sql: string; bindings: unknown[] } {
    const bindings: unknown[] = [];
    const text = segments
      .map((seg) => {
        if ("Literal" in seg) {
          return seg.Literal;
        }
        const vals = chunks[seg.Bind];
        bindings.push(...vals);
        return Array(vals.length).fill("?").join(", ");
      })
      .join("");
    return { sql: text, bindings };
  }

  function cartesian<T>(lists: T[][]): T[][] {
    return lists.reduce<T[][]>(
      (acc, list) => acc.flatMap((prefix) => list.map((item) => [...prefix, item])),
      [[]],
    );
  }
}

/** Order-preserving distinct over raw scalar identity. */
function distinct(values: unknown[]): unknown[] {
  const seen = new Set<unknown>();
  return values.filter((v) => (seen.has(v) ? false : (seen.add(v), true)));
}

function arg(raw: SelectArg): Arg {
  if ("Param" in raw) {
    return { kind: "param", name: raw.Param };
  }
  return { kind: "field", table: raw.Field.table, field: raw.Field.field };
}

/** Bucket key for a null/undefined join value, kept apart from any real value. */
const NULL_KEY = Symbol("null");

/**
 * Index the parent attachments by their join keys, returning a lookup from a child row to
 * its matching parent indices. A single join key buckets by the raw value (null/undefined
 * folded to a sentinel); multiple keys bucket by the JSON-encoded tuple.
 */
function bucketParents(
  parents: Attachment[],
  join: JoinKeys[],
): (row: Record<string, unknown>) => number[] {
  const push = <K>(buckets: Map<K, number[]>, k: K, i: number) => {
    const bucket = buckets.get(k);
    bucket ? bucket.push(i) : buckets.set(k, [i]);
  };

  if (join.length === 1) {
    const { parent_key, child_key } = join[0];
    const buckets = new Map<unknown, number[]>();
    parents.forEach((a, i) => push(buckets, a.value[parent_key] ?? NULL_KEY, i));
    return (row) => buckets.get(row[child_key] ?? NULL_KEY) ?? [];
  }

  const joinKey = (row: Record<string, unknown>, side: (j: JoinKeys) => string) =>
    JSON.stringify(join.map((j) => row[side(j)] ?? null));
  const buckets = new Map<string, number[]>();
  parents.forEach((a, i) =>
    push(
      buckets,
      joinKey(a.value, (j) => j.parent_key),
      i,
    ),
  );
  return (row) => buckets.get(joinKey(row, (j) => j.child_key)) ?? [];
}
