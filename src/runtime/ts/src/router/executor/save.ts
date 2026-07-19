import { interpolate, sinkResult, stepError, StorageResolver, templateArgs } from ".";
import { CloesceErrorKind, CloesceResult } from "../../common";
import type {
  Database,
  PathSegment,
  SaveArg,
  SavePlan,
  SaveStep,
  SqlStatement,
  TemplateSegment,
} from "./plan";

/** A step's output, deferred to the sequential sink: body attachments or a synthesize. */
type Sunk = { kind: "attach"; entries: [PathSegment[], unknown][] } | { kind: "synthesize" };

/** The storage a step targets, for error typing; Synthesize steps touch none. */
function database(step: SaveStep): Database | null {
  const q = step.query;
  if ("SqlBatch" in q) {
    return q.SqlBatch.database;
  }
  if ("KeyWrite" in q) {
    return q.KeyWrite.database;
  }
  return null;
}

export async function execute(
  plan: SavePlan,
  storage: StorageResolver,
): Promise<CloesceResult<any>> {
  const errors = [] as CloesceErrorKind[];
  const exec = new Executor(storage);

  for (const stage of plan.stages) {
    const settled = await Promise.allSettled(stage.steps.map((s) => exec.step(s)));
    settled.forEach((res, i) => {
      const step = stage.steps[i];
      try {
        if (res.status === "fulfilled") {
          exec.sink(step, res.value);
        } else {
          throw res.reason;
        }
      } catch (e) {
        errors.push(stepError(database(step), e));
      }
    });
  }
  return sinkResult(exec.body.value(), errors);
}

/** The working save body */
class WorkingResultBody {
  private root: any = null;

  value(): any {
    return this.root;
  }

  /**
   * Resolve a statement argument: a payload literal or a hydrated-body reference.
   */
  resolve(arg: SaveArg): unknown | undefined {
    return "Payload" in arg ? arg.Payload : this.valueAt(arg.Result);
  }

  /** The value at `path`, or `undefined` when absent. */
  valueAt(path: PathSegment[]): unknown | undefined {
    return path.reduce<any>(
      (cur, seg) => cur?.["Field" in seg ? seg.Field : seg.Index],
      this.root ?? undefined,
    );
  }

  /**
   * Place `value` at an exact path, creating intermediate objects/arrays on demand.
   *
   * An empty path merges onto an existing root object rather than replacing it.
   */
  attach(path: PathSegment[], value: unknown): void {
    if (path.length === 0) {
      if (isObject(this.root) && isObject(value)) {
        Object.assign(this.root, value);
      } else {
        this.root = value;
      }
      return;
    }

    if (!isContainer(this.root, path[0])) {
      this.root = emptyFor(path[0]);
    }

    let cur = this.root;
    for (let i = 0; i < path.length - 1; i++) {
      cur = WorkingResultBody.descend(cur, path[i], path[i + 1]);
    }
    WorkingResultBody.place(cur, path[path.length - 1], value);
  }

  /** Merge fields onto the object already at `path`; an absent slot is left untouched. */
  merge(path: PathSegment[], fields: Record<string, unknown>): void {
    const target = this.valueAt(path);
    if (isObject(target)) {
      Object.assign(target, fields);
    }
  }

  /** Step into (creating if needed) the child at `seg`; `next` decides its container kind. */
  private static descend(cur: any, seg: PathSegment, next: PathSegment): any {
    if ("Field" in seg) {
      if (!isContainer(cur[seg.Field], next)) {
        cur[seg.Field] = emptyFor(next);
      }
      return cur[seg.Field];
    }
    while (cur.length <= seg.Index) {
      cur.push(null);
    }
    if (!isContainer(cur[seg.Index], next)) {
      cur[seg.Index] = emptyFor(next);
    }
    return cur[seg.Index];
  }

  private static place(cur: any, seg: PathSegment, value: unknown): void {
    if ("Field" in seg) {
      cur[seg.Field] = value;
      return;
    }
    while (cur.length <= seg.Index) {
      cur.push(null);
    }
    cur[seg.Index] = value;
  }
}

class Executor {
  public body = new WorkingResultBody();
  constructor(private storage: StorageResolver) {}

  async step(step: SaveStep): Promise<Sunk> {
    const q = step.query;
    if ("SqlBatch" in q) {
      return { kind: "attach", entries: await this.batch(q.SqlBatch) };
    }
    if ("KeyWrite" in q) {
      return {
        kind: "attach",
        entries: await this.keyWrite(step.result, q.KeyWrite),
      };
    }
    return { kind: "synthesize" };
  }

  sink(step: SaveStep, out: Sunk): void {
    if (out.kind === "attach") {
      for (const [path, value] of out.entries) {
        this.body.attach(path, value);
      }
      return;
    }
    const q = step.query;
    if ("Synthesize" in q) {
      this.synthesize(step.result, q.Synthesize);
    }
  }

  /** Run one batch transactionally, returning each Hydrate read-back keyed by its path. */
  private async batch(q: {
    database: Database;
    statements: SqlStatement[];
    shard: [string, SaveArg][];
  }): Promise<[PathSegment[], unknown][]> {
    const tags = q.shard.map(([field, a]) => [field, this.body.resolve(a)] as const);
    const statements = q.statements.map((s) => {
      const spec = "Write" in s ? s.Write : s.Hydrate;
      return {
        sql: spec.sql,
        bindings: spec.arguments.map((a) => this.body.resolve(a)),
      };
    });

    if (
      tags.some(([, v]) => v === undefined) ||
      statements.some((s) => s.bindings.some((b) => b === undefined))
    ) {
      // A missing referenced value means an earlier step never produced the data
      // this batch depends on; skip the whole batch.
      return [];
    }

    const store = this.storage.sql(
      q.database,
      tags.map(([, v]) => v),
    );
    const results = await store.batch(statements);

    return q.statements.flatMap((s, i): [PathSegment[], unknown][] => {
      if (!("Hydrate" in s)) {
        return [];
      }
      const row = results[i]?.[0];
      if (!row) {
        // The read-back matched nothing; attach nothing.
        return [];
      }
      // Stamp shard fields so the body carries the row's full routing identity.
      return [[s.Hydrate.result, { ...row, ...Object.fromEntries(tags) }]];
    });
  }

  /** Write one key, returning the stored value for attachment at the step's result. */
  private async keyWrite(
    result: PathSegment[],
    q: {
      database: Database;
      segments: TemplateSegment<SaveArg>[];
      value: unknown;
      metadata: unknown | null;
      shard: [string, SaveArg][];
    },
  ): Promise<[PathSegment[], unknown][]> {
    const shard = q.shard.map(([, a]) => this.body.resolve(a));
    const keyValues = templateArgs(q.segments).map((a) => this.body.resolve(a));

    if ([...shard, ...keyValues].some((v) => v === undefined)) {
      // A missing shard or key value means the write has nothing to address; skip it.
      return [];
    }

    const key = interpolate(q.segments, keyValues);
    await this.storage.key(q.database, shard).put(key, q.value, q.metadata ?? undefined);
    return [[result, q.value]];
  }

  /**
   * `create` materializes a fresh value at the path; otherwise fields merge onto the
   * object already there (an absent slot is left untouched). Field resolution runs in
   * the sink so it sees the stage's earlier attachments.
   */
  private synthesize(
    result: PathSegment[],
    q: {
      fields: [string, SaveArg][];
      create: boolean;
      cardinality: "One" | "Many";
    },
  ): void {
    // A field whose reference is absent is left off rather than set to undefined.
    const buildFields = () =>
      Object.fromEntries(
        q.fields.flatMap(([field, a]) => {
          const value = this.body.resolve(a);
          return value === undefined ? [] : [[field, value]];
        }),
      );

    if (q.create) {
      const value =
        q.cardinality === "One" ? buildFields() : q.fields.length === 0 ? [] : [buildFields()];
      this.body.attach(result, value);
      return;
    }

    this.body.merge(result, buildFields());
  }
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
