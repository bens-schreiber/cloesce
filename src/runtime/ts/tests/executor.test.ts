import { describe, test, expect } from "vitest";
import {
  executeSelect,
  executeSave,
  MAX_BOUND_PARAMETERS,
  type KeyStore,
  type KeyValueWrapper,
  type SqlStore,
  type StorageResolver,
} from "../src/router/executor.js";
import type {
  Database,
  MapCardinality,
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
} from "../src/router/plan.js";
import { KValue } from "../src/ui/backend.js";

type QueryCall = { sql: string; bindings: unknown[] };
type BatchCall = { statements: QueryCall[] };

class MockSqlStore implements SqlStore {
  queries: QueryCall[] = [];
  batches: BatchCall[] = [];

  constructor(
    private queryResponder: (call: QueryCall) => Record<string, unknown>[] = () => [],
    private batchResponder: (call: BatchCall) => Record<string, unknown>[][] = () => [],
  ) {}

  async query(sql: string, bindings: unknown[]): Promise<Record<string, unknown>[]> {
    const call = { sql, bindings };
    this.queries.push(call);
    return this.queryResponder(call);
  }

  async batch(statements: QueryCall[]): Promise<Record<string, unknown>[][]> {
    const call = { statements };
    this.batches.push(call);
    return this.batchResponder(call);
  }
}

class MockKeyStore implements KeyStore {
  gets: string[] = [];
  puts: { key: string; value: unknown; metadata: unknown }[] = [];

  constructor(private store: Map<string, unknown> = new Map()) {}

  get(key: string): unknown {
    this.gets.push(key);
    return this.store.has(key) ? this.store.get(key) : null;
  }

  put(key: string, value: unknown, metadata?: unknown): void {
    this.puts.push({ key, value, metadata });
    this.store.set(key, value);
  }
}

/**
 * A mock {@link StorageResolver}. `sql`/`key` are resolved by a `(database, shard) -> store`
 * lookup; every distinct `(name, shard)` gets its own recorded store so shard fan-out and
 * routing are observable.
 */
class MockResolver implements StorageResolver {
  sqlStores = new Map<string, MockSqlStore>();
  keyStores = new Map<string, MockKeyStore>();

  constructor(
    private sqlFactory: (database: Database, shard: unknown[]) => MockSqlStore = () =>
      new MockSqlStore(),
    private keyFactory: (database: Database, shard: unknown[]) => MockKeyStore = () =>
      new MockKeyStore(),
  ) {}

  private slot(database: Database, shard: unknown[]): string {
    return `${database.name}|${JSON.stringify(shard)}`;
  }

  sql(database: Database, shard: unknown[]): SqlStore {
    const slot = this.slot(database, shard);
    if (!this.sqlStores.has(slot)) {
      this.sqlStores.set(slot, this.sqlFactory(database, shard));
    }
    return this.sqlStores.get(slot)!;
  }

  key(database: Database, shard: unknown[]): KeyStore {
    const slot = this.slot(database, shard);
    if (!this.keyStores.has(slot)) {
      this.keyStores.set(slot, this.keyFactory(database, shard));
    }
    return this.keyStores.get(slot)!;
  }
}

function d1(name = "db"): Database {
  return { name, kind: "D1" };
}
function doDb(name = "doDb"): Database {
  return { name, kind: "DurableObject" };
}
function kvDb(name = "kv"): Database {
  return { name, kind: "Kv" };
}
function r2Db(name = "r2"): Database {
  return { name, kind: "R2" };
}

const one: Mapping = { cardinality: "One", join: [] };
const many: Mapping = { cardinality: "Many", join: [] };
function mapping(cardinality: MapCardinality, join: Mapping["join"] = []): Mapping {
  return {
    cardinality,
    join,
  };
}

// A Sql `arguments` slot is a SelectArg: a `Param` binds one value, a `Result` path spreads
// the distinct values of that owner-table field across its rows into an `IN (...)` list.
function param(name: string): SelectArg {
  return { Param: name };
}
function spread(...path: string[]): SelectArg {
  return { Result: path };
}

/** Each variadic argument is one stage's steps. */
function selectPlan(...stages: SelectStep[][]): SelectPlan {
  return {
    stages: stages.map((steps) => ({ steps })),
  };
}

function sqlStep(
  result: string[],
  sql: string,
  opts: {
    args?: SelectArg[];
    mapping?: Mapping;
    db?: Database;
    shard?: [string, SelectArg][];
  } = {},
): SelectStep {
  return {
    result,
    query: {
      Sql: {
        database: opts.db ?? d1(),
        sql,
        arguments: opts.args ?? [],
        mapping: opts.mapping ?? one,
        shard: opts.shard ?? [],
      },
    },
  };
}

function keyStep(
  result: string[],
  database: Database,
  segments: TemplateSegment<SelectArg>[],
  shard: [string, SelectArg][] = [],
): SelectStep {
  return { result, query: { Key: { database, segments, shard } } };
}

function synthStep(
  result: string[],
  fields: [string, SelectArg][],
  cardinality: MapCardinality = "One",
): SelectStep {
  return { result, query: { Synthesize: { fields, cardinality } } };
}

/** Run a select expected to sink no errors, returning the hydrated body. */
async function executeSelectOk(...args: Parameters<typeof executeSelect>): Promise<any> {
  const res = await executeSelect(...args);
  expect(res.errors).toEqual([]);
  return res.value;
}

/** Run a save expected to sink no errors, returning the saved body. */
async function executeSaveOk(...args: Parameters<typeof executeSave>): Promise<any> {
  const res = await executeSave(...args);
  expect(res.errors).toEqual([]);
  return res.value;
}

/** Match a sunk error of `kind` whose Error message matches `message`. */
function sunkError(kind: string, message: RegExp) {
  return expect.objectContaining({
    kind,
    error: expect.objectContaining({ message: expect.stringMatching(message) }),
  });
}

const passthroughWrap: KeyValueWrapper = (_db, _path, raw) => raw ?? null;
const kValueWrap: KeyValueWrapper = (database, _path, raw, metadata) => {
  if (database.kind === "Kv") {
    const inner = (raw ?? {}) as { value?: unknown; metadata?: unknown };
    return new KValue(inner.value ?? null, inner.metadata ?? metadata ?? null);
  }
  return raw ?? null;
};

describe("executeSelect cardinality", () => {
  test("One cardinality maps the first row to a root object", async () => {
    const plan = selectPlan([
      sqlStep([], 'SELECT * FROM "M" WHERE "id" = ?1', { args: [param("id")] }),
    ]);
    const resolver = new MockResolver(
      () =>
        new MockSqlStore(() => [
          { id: 1, name: "a" },
          { id: 1, name: "b" },
        ]),
    );

    const body = await executeSelectOk(plan, { id: 1 }, resolver, passthroughWrap);

    expect(body).toEqual({ id: 1, name: "a" });
    expect(resolver.sqlStores.get("db|[]")!.queries).toEqual([
      { sql: 'SELECT * FROM "M" WHERE "id" = ?', bindings: [1] },
    ]);
  });

  test("One cardinality yields null when no rows come back", async () => {
    const plan = selectPlan([sqlStep([], "SELECT 1")]);
    const body = await executeSelectOk(plan, {}, new MockResolver(), passthroughWrap);
    expect(body).toBeNull();
  });

  test("Many cardinality maps all rows to a root array", async () => {
    const plan = selectPlan([sqlStep([], "SELECT * FROM M", { mapping: many })]);
    const rows = [{ id: 1 }, { id: 2 }];
    const resolver = new MockResolver(() => new MockSqlStore(() => rows));

    const body = await executeSelectOk(plan, {}, resolver, passthroughWrap);
    expect(body).toEqual([{ id: 1 }, { id: 2 }]);
  });
});

describe("executeSelect nested include joins", () => {
  test("attaches Many children as an array on each parent via join keys", async () => {
    const parents = [{ id: 1 }, { id: 2 }];
    const children = [
      { id: 10, parentId: 1 },
      { id: 11, parentId: 1 },
      { id: 12, parentId: 2 },
    ];
    const resolver = new MockResolver((database) =>
      database.name === "parents"
        ? new MockSqlStore(() => parents)
        : new MockSqlStore(() => children),
    );
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM parents", { db: d1("parents"), mapping: many })],
      [
        sqlStep(["children"], 'SELECT * FROM children WHERE "parentId" IN (?1)', {
          db: d1("children"),
          args: [spread("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
    );

    const body = await executeSelectOk(plan, {}, resolver, passthroughWrap);

    expect(body).toEqual([
      {
        id: 1,
        children: [
          { id: 10, parentId: 1 },
          { id: 11, parentId: 1 },
        ],
      },
      { id: 2, children: [{ id: 12, parentId: 2 }] },
    ]);
  });

  test("attaches a One child as an object on the parent", async () => {
    const resolver = new MockResolver((database) =>
      database.name === "root"
        ? new MockSqlStore(() => [{ id: 1, ownerId: 7 }])
        : new MockSqlStore(() => [{ id: 7, name: "owner" }]),
    );
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root") })],
      [
        sqlStep(["owner"], 'SELECT * FROM owners WHERE "id" IN (?1)', {
          db: d1("owners"),
          args: [spread("ownerId")],
          mapping: mapping("One", [{ parent_key: "ownerId", child_key: "id" }]),
        }),
      ],
    );

    const body = await executeSelectOk(plan, {}, resolver, passthroughWrap);
    expect(body).toEqual({ id: 1, ownerId: 7, owner: { id: 7, name: "owner" } });
  });
});

describe("executeSelect spread IN expansion", () => {
  /** A root list stage plus one child stage joining `children.parentId -> root.id`. */
  const childPlan = (childArgs: SelectArg[], childSql: string): SelectPlan =>
    selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["children"], childSql, {
          db: d1("children"),
          args: childArgs,
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
    );

  test("expands a Spread into one placeholder per deduped value", async () => {
    const parents = [{ id: 1 }, { id: 2 }, { id: 2 }, { id: 3 }];
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : new MockSqlStore(() => []),
    );

    await executeSelectOk(
      childPlan([spread("id")], 'SELECT * FROM children WHERE "parentId" IN (?1)'),
      {},
      resolver,
      passthroughWrap,
    );

    expect(resolver.sqlStores.get("children|[]")!.queries).toEqual([
      { sql: 'SELECT * FROM children WHERE "parentId" IN (?, ?, ?)', bindings: [1, 2, 3] },
    ]);
  });

  test("renumbers correctly when a Param arg follows the Spread", async () => {
    const parents = [{ id: 1 }, { id: 2 }];
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : new MockSqlStore(() => []),
    );

    await executeSelectOk(
      childPlan(
        [spread("id"), param("kind")],
        'SELECT * FROM children WHERE "parentId" IN (?1) AND "kind" = ?2',
      ),
      { kind: "active" },
      resolver,
      passthroughWrap,
    );

    expect(resolver.sqlStores.get("children|[]")!.queries).toEqual([
      {
        sql: 'SELECT * FROM children WHERE "parentId" IN (?, ?) AND "kind" = ?',
        bindings: [1, 2, "active"],
      },
    ]);
  });

  test("a zero-element spread short-circuits to empty rows without hitting the store", async () => {
    const childStore = new MockSqlStore(() => [{ id: 99 }]);
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => [{ id: 1 }]) : childStore,
    );

    // `missing` is absent from every parent, so the spread is empty.
    const body = await executeSelectOk(
      childPlan([spread("missing")], 'SELECT * FROM children WHERE "parentId" IN (?1)'),
      {},
      resolver,
      passthroughWrap,
    );

    expect(body).toEqual([{ id: 1, children: [] }]);
    expect(childStore.queries).toEqual([]);
  });

  test("expands ?1 vs ?10 without a naive string-replace collision", async () => {
    // Ten single-value params: only ?1 and ?10 differ by a prefix; a naive replace of `?1`
    // would corrupt `?10`. `IN (?1)` must stay one placeholder and `?10` its own.
    const args: SelectArg[] = Array.from({ length: 10 }, (_, i) => param(`p${i + 1}`));
    const params = Object.fromEntries(args.map((_, i) => [`p${i + 1}`, i + 1]));
    const store = new MockSqlStore(() => [{ ok: 1 }]);
    const resolver = new MockResolver(() => store);
    const plan = selectPlan([
      sqlStep([], "SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10", { args }),
    ]);

    await executeSelectOk(plan, params, resolver, passthroughWrap);

    expect(store.queries).toEqual([
      { sql: "SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?", bindings: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] },
    ]);
  });
});

describe("executeSelect spread chunking (MAX_BOUND_PARAMETERS)", () => {
  /** Build a two-stage plan whose child SQL spreads `id` over `?1`, plus optional fixed params. */
  const chunkPlan = (childArgs: SelectArg[], childSql: string): SelectPlan =>
    selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["children"], childSql, {
          db: d1("children"),
          args: childArgs,
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
    );

  test("exactly at the limit issues a single query and no batch", async () => {
    const parents = Array.from({ length: MAX_BOUND_PARAMETERS }, (_, i) => ({ id: i + 1 }));
    const childStore = new MockSqlStore(() => []);
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : childStore,
    );

    await executeSelectOk(
      chunkPlan([spread("id")], 'SELECT * FROM children WHERE "parentId" IN (?1)'),
      {},
      resolver,
      passthroughWrap,
    );

    expect(childStore.queries.length).toBe(1);
    expect(childStore.batches.length).toBe(0);
    expect(childStore.queries[0].bindings.length).toBe(MAX_BOUND_PARAMETERS);
  });

  test("over the limit chunks into one batch whose statements each stay within budget and whose union equals the unchunked result", async () => {
    const count = MAX_BOUND_PARAMETERS + 50; // 150 distinct ids -> 2 chunks of 100 + 50
    const parents = Array.from({ length: count }, (_, i) => ({ id: i + 1 }));
    // Each child row echoes its parentId so we can verify the union covers every id.
    const childStore = new MockSqlStore(
      () => [],
      (call) =>
        call.statements.map((s) =>
          s.bindings.map((pid) => ({ id: 1000 + (pid as number), parentId: pid })),
        ),
    );
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : childStore,
    );

    const body = await executeSelectOk(
      chunkPlan([spread("id")], 'SELECT * FROM children WHERE "parentId" IN (?1)'),
      {},
      resolver,
      passthroughWrap,
    );

    // Exactly one batch, no single-query path.
    expect(childStore.batches.length).toBe(1);
    expect(childStore.queries.length).toBe(0);
    const stmts = childStore.batches[0].statements;
    expect(stmts.length).toBe(2);
    for (const s of stmts) {
      expect(s.bindings.length).toBeLessThanOrEqual(MAX_BOUND_PARAMETERS);
    }
    // Chunks are disjoint and their union is all ids.
    const allBindings = stmts.flatMap((s) => s.bindings);
    expect(allBindings.length).toBe(count);
    expect(new Set(allBindings).size).toBe(count);
    // Each parent got its one echoed child attached.
    expect(
      (body as any[]).every((p) => p.children.length === 1 && p.children[0].parentId === p.id),
    ).toBe(true);
  });

  test("fixed params are re-bound in every chunk", async () => {
    const count = MAX_BOUND_PARAMETERS + 5;
    const parents = Array.from({ length: count }, (_, i) => ({ id: i + 1 }));
    const childStore = new MockSqlStore(() => []);
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : childStore,
    );

    // One fixed param leaves a budget of 99 for the spread -> chunks of 99, 6.
    await executeSelectOk(
      chunkPlan(
        [spread("id"), param("kind")],
        'SELECT * FROM children WHERE "parentId" IN (?1) AND "kind" = ?2',
      ),
      { kind: "active" },
      resolver,
      passthroughWrap,
    );

    const stmts = childStore.batches[0].statements;
    expect(stmts.length).toBe(2);
    for (const s of stmts) {
      // Fixed param re-bound as the last binding of every chunk.
      expect(s.bindings[s.bindings.length - 1]).toBe("active");
      expect(s.bindings.length).toBeLessThanOrEqual(MAX_BOUND_PARAMETERS);
      expect(s.sql).toContain('"kind" = ?');
    }
  });

  test("multiple spreads chunk as a cross-product covering every pair", async () => {
    // Two spreads share the budget (50 each). 60 ids x 60 tags -> 2 x 2 = 4 chunk queries.
    const n = 60;
    const parents = Array.from({ length: n }, (_, i) => ({ id: i + 1, tag: i + 1 }));
    const childStore = new MockSqlStore(
      () => [],
      (call) => call.statements.map(() => []),
    );
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : childStore,
    );

    await executeSelectOk(
      chunkPlan(
        [spread("id"), spread("tag")],
        'SELECT * FROM children WHERE "parentId" IN (?1) AND "tagId" IN (?2)',
      ),
      {},
      resolver,
      passthroughWrap,
    );

    const stmts = childStore.batches[0].statements;
    expect(stmts.length).toBe(4); // 2 id-chunks x 2 tag-chunks
    for (const s of stmts) {
      expect(s.bindings.length).toBeLessThanOrEqual(MAX_BOUND_PARAMETERS);
    }
    // Union of (id-chunk x tag-chunk) covers every id and every tag value.
    const idsSeen = new Set<number>();
    const tagsSeen = new Set<number>();
    for (const s of stmts) {
      // First half of bindings are ids, remainder are tags, per slot order.
      const idCount = (s.sql.match(/IN \(([^)]*)\)/g)?.[0].match(/\?/g) ?? []).length;
      s.bindings.slice(0, idCount).forEach((v) => idsSeen.add(v as number));
      s.bindings.slice(idCount).forEach((v) => tagsSeen.add(v as number));
    }
    expect(idsSeen.size).toBe(n);
    expect(tagsSeen.size).toBe(n);
  });
});

describe("executeSelect DO shard fan-out", () => {
  test("fans out to each distinct shard tuple and tags rows, deduping identical shards", async () => {
    const parents = [
      { id: 1, tenant: "A" },
      { id: 2, tenant: "A" },
      { id: 3, tenant: "B" },
    ];
    const resolver = new MockResolver((database, shard) => {
      if (database.name === "root") return new MockSqlStore(() => parents);
      // Each shard stub returns one row keyed by its tenant.
      const tenant = shard[0];
      return new MockSqlStore(() => [{ note: `n-${tenant}` }]);
    });
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["notes"], "SELECT * FROM notes", {
          db: doDb("notes"),
          mapping: mapping("Many", [{ parent_key: "tenant", child_key: "tenant" }]),
          shard: [["tenant", spread("tenant")]],
        }),
      ],
    );

    const body = await executeSelectOk(plan, {}, resolver, passthroughWrap);

    // Two distinct tenants -> two stubs hit (A deduped from two parents), rows shard-tagged.
    expect(resolver.sqlStores.has('notes|["A"]')).toBe(true);
    expect(resolver.sqlStores.has('notes|["B"]')).toBe(true);
    expect(resolver.sqlStores.size).toBe(3); // root + two shards
    expect(body).toEqual([
      { id: 1, tenant: "A", notes: [{ note: "n-A", tenant: "A" }] },
      { id: 2, tenant: "A", notes: [{ note: "n-A", tenant: "A" }] },
      { id: 3, tenant: "B", notes: [{ note: "n-B", tenant: "B" }] },
    ]);
  });
});

describe("executeSelect Key steps", () => {
  test("KV read is placed at the result path and wrapped as a KValue", async () => {
    const kvBacking = new Map<string, unknown>([
      ["profile:1", { value: { bio: "hi" }, metadata: { v: 2 } }],
    ]);
    const resolver = new MockResolver(
      () => new MockSqlStore(() => [{ id: 1 }]),
      () => new MockKeyStore(kvBacking),
    );
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM M")],
      [keyStep(["profile"], kvDb(), [{ Literal: "profile:" }, { Value: spread("id") }])],
    );

    const body = await executeSelectOk(plan, {}, resolver, kValueWrap);

    expect(body.profile).toBeInstanceOf(KValue);
    expect(body.profile.value).toEqual({ bio: "hi" });
    expect(body.profile.metadata).toEqual({ v: 2 });
    expect(resolver.keyStores.get("kv|[]")!.gets).toEqual(["profile:1"]);
  });

  test("R2-style read passes through raw with the passthrough wrapper", async () => {
    const r2Backing = new Map<string, unknown>([["blob:1", { bytes: [1, 2, 3] }]]);
    const resolver = new MockResolver(
      () => new MockSqlStore(() => [{ id: 1 }]),
      () => new MockKeyStore(r2Backing),
    );
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM M")],
      [keyStep(["blob"], r2Db(), [{ Literal: "blob:" }, { Value: spread("id") }])],
    );

    const body = await executeSelectOk(plan, {}, resolver, passthroughWrap);
    expect(body.blob).toEqual({ bytes: [1, 2, 3] });
  });
});

describe("executeSelect Synthesize steps", () => {
  test("materializes a root object from params when no buffer exists at the path yet", async () => {
    const plan = selectPlan([
      synthStep(
        [],
        [
          ["id", param("id")],
          ["name", param("name")],
        ],
      ),
    ]);

    const body = await executeSelectOk(
      plan,
      { id: 5, name: "z" },
      new MockResolver(),
      passthroughWrap,
    );
    expect(body).toEqual({ id: 5, name: "z" });
  });

  test("merges synthesized fields onto the buffer an earlier step already produced at the path", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(() => [{ id: 1 }, { id: 2 }]));
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM M", { mapping: many })],
      [synthStep([], [["tag", param("tag")]], "Many")],
    );

    const body = await executeSelectOk(plan, { tag: "t" }, resolver, passthroughWrap);
    expect(body).toEqual([
      { id: 1, tag: "t" },
      { id: 2, tag: "t" },
    ]);
  });

  test("materializes a child object under a parent from params and parent fields", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(() => [{ id: 1, ownerId: 7 }]));
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM M")],
      [
        synthStep(
          ["owner"],
          [
            ["id", spread("ownerId")],
            ["label", param("label")],
          ],
        ),
      ],
    );

    const body = await executeSelectOk(plan, { label: "L" }, resolver, passthroughWrap);
    expect(body).toEqual({ id: 1, ownerId: 7, owner: { id: 7, label: "L" } });
  });
});

describe("executeSelect buffer merge + assemble ordering", () => {
  test("a Synthesize sharing a nested path merges onto the buffer produced by an earlier step", async () => {
    // The root Sql produces two rows; a child Sql materializes a One buffer at `owner`;
    // a later Synthesize on the SAME `owner` path merges a field onto each owner rather
    // than materializing a fresh buffer (buffer already present ⇒ merge).
    const resolver = new MockResolver((database) => {
      if (database.name === "root") return new MockSqlStore(() => [{ id: 1, ownerId: 7 }]);
      return new MockSqlStore(() => [{ id: 7, name: "o" }]);
    });
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root") })],
      [
        sqlStep(["owner"], 'SELECT * FROM owners WHERE "id" IN (?1)', {
          db: d1("owners"),
          args: [spread("ownerId")],
          mapping: mapping("One", [{ parent_key: "ownerId", child_key: "id" }]),
        }),
        // Same `owner` path: merges `role` onto the existing owner buffer.
        synthStep(["owner"], [["role", param("role")]]),
      ],
    );

    const body = await executeSelectOk(plan, { role: "admin" }, resolver, passthroughWrap);
    expect(body).toEqual({ id: 1, ownerId: 7, owner: { id: 7, name: "o", role: "admin" } });
  });

  test("assembles deepest paths first: a grandchild lands inside its child before the child attaches", async () => {
    const resolver = new MockResolver((database) => {
      if (database.name === "a") return new MockSqlStore(() => [{ id: 1 }]);
      if (database.name === "b") return new MockSqlStore(() => [{ id: 10, aId: 1 }]);
      return new MockSqlStore(() => [{ id: 100, bId: 10 }]);
    });
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM a", { db: d1("a") })],
      [
        sqlStep(["b"], 'SELECT * FROM b WHERE "aId" IN (?1)', {
          db: d1("b"),
          args: [spread("id")],
          mapping: mapping("One", [{ parent_key: "id", child_key: "aId" }]),
        }),
      ],
      [
        sqlStep(["b", "c"], 'SELECT * FROM c WHERE "bId" IN (?1)', {
          db: d1("c"),
          args: [spread("b", "id")],
          mapping: mapping("One", [{ parent_key: "id", child_key: "bId" }]),
        }),
      ],
    );

    const body = await executeSelectOk(plan, {}, resolver, passthroughWrap);
    expect(body).toEqual({ id: 1, b: { id: 10, aId: 1, c: { id: 100, bId: 10 } } });
  });
});

describe("executeSelect seek pagination", () => {
  test("binds lastSeen_* and limit params into the SQL args", async () => {
    const store = new MockSqlStore(() => [{ id: 6 }]);
    const resolver = new MockResolver(() => store);
    const plan = selectPlan([
      sqlStep([], 'SELECT * FROM M WHERE "id" > ?1 ORDER BY "id" LIMIT ?2', {
        args: [param("lastSeen_id"), param("limit")],
        mapping: many,
      }),
    ]);

    await executeSelectOk(plan, { lastSeen_id: 5, limit: 20 }, resolver, passthroughWrap);

    expect(store.queries).toEqual([
      {
        sql: 'SELECT * FROM M WHERE "id" > ? ORDER BY "id" LIMIT ?',
        bindings: [5, 20],
      },
    ]);
  });
});

describe("executeSelect stage/step parallelism", () => {
  /** A promise plus its resolve, so a test can control exactly when a mock call settles. */
  function deferred<T>(): { promise: Promise<T>; resolve: (v: T) => void } {
    let resolve!: (v: T) => void;
    const promise = new Promise<T>((r) => (resolve = r));
    return { promise, resolve };
  }

  test("a Key step reads across its parents concurrently", async () => {
    const inFlight = new Set<string>();
    const maxConcurrent = { count: 0 };
    const deferredByKey = new Map<string, ReturnType<typeof deferred<unknown>>>();

    class TrackingKeyStore implements KeyStore {
      async get(key: string): Promise<unknown> {
        inFlight.add(key);
        maxConcurrent.count = Math.max(maxConcurrent.count, inFlight.size);
        const d = deferred<unknown>();
        deferredByKey.set(key, d);
        const value = await d.promise;
        inFlight.delete(key);
        return value;
      }
      put(): void {
        throw new Error("not used");
      }
    }

    const resolver = new MockResolver(
      () => new MockSqlStore(() => [{ id: 1 }, { id: 2 }]),
      () => new TrackingKeyStore() as unknown as MockKeyStore,
    );
    // A List root over two rows, then one Key step keyed off each row's `id`. The reads for
    // the two parents fan out in parallel within the step's sink.
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM M", { mapping: many })],
      [keyStep(["data"], kvDb("kv"), [{ Literal: "k/" }, { Value: spread("id") }])],
    );

    const resultPromise = executeSelectOk(plan, {}, resolver, passthroughWrap);

    for (let i = 0; i < 10 && maxConcurrent.count < 2; i++) {
      await Promise.resolve();
    }
    expect(maxConcurrent.count).toBe(2); // both parents' reads were in flight simultaneously

    deferredByKey.get("k/1")!.resolve("A");
    deferredByKey.get("k/2")!.resolve("B");
    const body = await resultPromise;
    expect(body).toEqual([
      { id: 1, data: "A" },
      { id: 2, data: "B" },
    ]);
  });

  test("a Key step in the same stage as its root Sql step sees the attached row", async () => {
    // The GET-with-KV shape: a root Sql step and a Key step keyed off a param, in ONE stage.
    // The Key attaches under the root object, which the Sql step produces earlier in the same
    // stage — so the step-ordered sink must run the Sql attach before the Key read/attach.
    const resolver = new MockResolver(
      () => new MockSqlStore(() => [{ id: 7 }]),
      () => new MockKeyStore(new Map([["k/7", "V"]])),
    );
    const plan = selectPlan([
      sqlStep([], "SELECT * FROM M WHERE id = ?1", { args: [param("id")] }),
      keyStep(["data"], kvDb("kv"), [{ Literal: "k/" }, { Value: param("id") }]),
    ]);

    const body = await executeSelectOk(plan, { id: 7 }, resolver, passthroughWrap);
    expect(body).toEqual({ id: 7, data: "V" });
  });

  test("a later stage's store call happens only after all of the prior stage's calls resolve", async () => {
    const events: string[] = [];
    const stage1 = deferred<Record<string, unknown>[]>();

    class OrderedSqlStore implements SqlStore {
      constructor(
        private label: string,
        private onQuery: () => Promise<Record<string, unknown>[]>,
      ) {}
      async query(): Promise<Record<string, unknown>[]> {
        events.push(`${this.label}:start`);
        const rows = await this.onQuery();
        events.push(`${this.label}:end`);
        return rows;
      }
      async batch(): Promise<Record<string, unknown>[][]> {
        throw new Error("not used");
      }
    }

    const rootStore = new OrderedSqlStore("root", () => stage1.promise);
    const childStore = new OrderedSqlStore("child", async () => [{ id: 99, parentId: 1 }]);
    const resolver = new MockResolver((database) =>
      database.name === "root"
        ? (rootStore as unknown as MockSqlStore)
        : (childStore as unknown as MockSqlStore),
    );
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["children"], 'SELECT * FROM children WHERE "parentId" IN (?1)', {
          db: d1("children"),
          args: [spread("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
    );

    const resultPromise = executeSelectOk(plan, {}, resolver, passthroughWrap);

    // Let stage 1's query start; stage 2 must not start until stage 1 resolves.
    await Promise.resolve();
    await Promise.resolve();
    expect(events).toEqual(["root:start"]);

    stage1.resolve([{ id: 1 }]);
    const body = await resultPromise;

    expect(events).toEqual(["root:start", "root:end", "child:start", "child:end"]);
    expect(body).toEqual([{ id: 1, children: [{ id: 99, parentId: 1 }] }]);
  });
});

describe("executeSelect error semantics", () => {
  test("a missing Param sinks a generic error and nulls the value", async () => {
    const plan = selectPlan([sqlStep([], "SELECT ?1", { args: [param("id")] })]);

    const res = await executeSelect(plan, {}, new MockResolver(), passthroughWrap);

    expect(res.value).toBeNull();
    expect(res.errors).toEqual([sunkError("generic", /missing parameter "id"/)]);
  });

  test("every failing step sinks, typed by its storage kind, and later stages still run", async () => {
    class ThrowingKeyStore implements KeyStore {
      get(): unknown {
        throw new Error("key store down");
      }
      put(): void {}
    }
    const lastStageStore = new MockSqlStore(() => []);
    const resolver = new MockResolver(
      (database) => {
        if (database.name === "root") return new MockSqlStore(() => [{ id: 1 }]);
        if (database.name === "broken")
          return new MockSqlStore(() => {
            throw new Error("sql down");
          });
        return lastStageStore;
      },
      () => new ThrowingKeyStore() as unknown as MockKeyStore,
    );
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        keyStep(["kvData"], kvDb(), [{ Literal: "k/" }, { Value: spread("id") }]),
        keyStep(["r2Data"], r2Db(), [{ Literal: "b/" }, { Value: spread("id") }]),
        sqlStep(["children"], 'SELECT * FROM broken WHERE "parentId" IN (?1)', {
          db: d1("broken"),
          args: [spread("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
      [
        sqlStep(["more"], 'SELECT * FROM more WHERE "parentId" IN (?1)', {
          db: d1("more"),
          args: [spread("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
    );

    const res = await executeSelect(plan, {}, resolver, passthroughWrap);

    // The surviving steps' partial body comes back: failed Many steps degrade to [],
    // failed One steps leave their slot absent.
    expect(res.value).toEqual([{ id: 1, children: [], more: [] }]);
    expect(res.errors).toEqual([
      sunkError("kv", /key store down/),
      sunkError("r2", /key store down/),
      sunkError("generic", /sql down/),
    ]);
    // The plan ran to completion: the final stage still hit its store.
    expect(lastStageStore.queries.length).toBe(1);
  });
});

describe("seeded select", () => {
  test("root-only seed skips the root fetch, needing no seek params", async () => {
    // A list plan whose root binds seek params; seeding the root must skip it entirely,
    // so those params are never demanded and the root store is never queried.
    const rootStore = new MockSqlStore(() => [{ id: 999 }]);
    const resolver = new MockResolver(() => rootStore);
    const plan = selectPlan([
      sqlStep([], 'SELECT * FROM M WHERE "id" > ?1 ORDER BY "id" LIMIT ?2', {
        args: [param("lastSeen_id"), param("limit")],
        mapping: many,
      }),
    ]);

    const body = await executeSelectOk(
      plan,
      {}, // no lastSeen_id / limit supplied
      resolver,
      passthroughWrap,
      [
        { id: 1, name: "a" },
        { id: 2, name: "b" },
      ],
    );

    expect(body).toEqual([
      { id: 1, name: "a" },
      { id: 2, name: "b" },
    ]);
    expect(rootStore.queries).toEqual([]);
  });

  test("root+child seed with a grandchild fetched off the seeded child values", async () => {
    // Root and its `children` are seeded; the grandchild `notes` is absent, so it is
    // fetched, spreading the seeded children's ids into the IN (...) query.
    const grandStore = new MockSqlStore(() => [
      { id: 100, childId: 10 },
      { id: 101, childId: 11 },
    ]);
    const rootStore = new MockSqlStore(() => {
      throw new Error("root must not be fetched");
    });
    const childStore = new MockSqlStore(() => {
      throw new Error("child must not be fetched");
    });
    const resolver = new MockResolver((database) => {
      if (database.name === "root") return rootStore;
      if (database.name === "children") return childStore;
      return grandStore;
    });
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["children"], 'SELECT * FROM children WHERE "parentId" IN (?1)', {
          db: d1("children"),
          args: [spread("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
      [
        sqlStep(["children", "notes"], 'SELECT * FROM notes WHERE "childId" IN (?1)', {
          db: d1("notes"),
          args: [spread("children", "id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "childId" }]),
        }),
      ],
    );

    const body = await executeSelectOk(plan, {}, resolver, passthroughWrap, [
      {
        id: 1,
        children: [
          { id: 10, parentId: 1 },
          { id: 11, parentId: 1 },
        ],
      },
    ]);

    expect(grandStore.queries).toEqual([
      { sql: 'SELECT * FROM notes WHERE "childId" IN (?, ?)', bindings: [10, 11] },
    ]);
    expect(body).toEqual([
      {
        id: 1,
        children: [
          { id: 10, parentId: 1, notes: [{ id: 100, childId: 10 }] },
          { id: 11, parentId: 1, notes: [{ id: 101, childId: 11 }] },
        ],
      },
    ]);
  });

  test("a seeded [] child skips its step and every downstream step", async () => {
    const childStore = new MockSqlStore(() => [{ id: 10, parentId: 1 }]);
    const grandStore = new MockSqlStore(() => [{ id: 100, childId: 10 }]);
    const resolver = new MockResolver((database) =>
      database.name === "children" ? childStore : grandStore,
    );
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["children"], 'SELECT * FROM children WHERE "parentId" IN (?1)', {
          db: d1("children"),
          args: [spread("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
      [
        sqlStep(["children", "notes"], 'SELECT * FROM notes WHERE "childId" IN (?1)', {
          db: d1("notes"),
          args: [spread("children", "id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "childId" }]),
        }),
      ],
    );

    const body = await executeSelectOk(plan, {}, resolver, passthroughWrap, [
      { id: 1, children: [] },
    ]);

    expect(childStore.queries).toEqual([]);
    expect(grandStore.queries).toEqual([]);
    expect(body).toEqual([{ id: 1, children: [] }]);
  });

  test("mixed presence of a seeded field degrades absent parents to empty", async () => {
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["children"], 'SELECT * FROM children WHERE "parentId" IN (?1)', {
          db: d1("children"),
          args: [spread("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
    );

    const res = await executeSelect(plan, {}, new MockResolver(), passthroughWrap, [
      { id: 1, children: [{ id: 10, parentId: 1 }] },
      { id: 2 }, // children absent here
    ]);

    expect(res.errors).toEqual([]);
    expect(res.value).toEqual([
      { id: 1, children: [{ id: 10, parentId: 1 }] },
      { id: 2, children: [] },
    ]);
  });

  test("a One-cardinality null seed leaves the slot null and skips the fetch", async () => {
    const ownerStore = new MockSqlStore(() => [{ id: 7, name: "owner" }]);
    const resolver = new MockResolver((database) =>
      database.name === "owners" ? ownerStore : new MockSqlStore(() => []),
    );
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["owner"], 'SELECT * FROM owners WHERE "id" IN (?1)', {
          db: d1("owners"),
          args: [spread("ownerId")],
          mapping: mapping("One", [{ parent_key: "ownerId", child_key: "id" }]),
        }),
      ],
    );

    const body = await executeSelectOk(plan, {}, resolver, passthroughWrap, [
      { id: 1, ownerId: 7, owner: null },
    ]);

    expect(ownerStore.queries).toEqual([]);
    expect(body).toEqual([{ id: 1, ownerId: 7, owner: null }]);
  });

  test("a DO-sharded root seed stamps Param shard fields, routing a same-stage child fetch", async () => {
    // Root and its same-stage `notes` child share stage 0: the child's shard reads the root
    // row's `tenant`, which the seed omits, so plantSeeds stamps it from the tenantId param
    // and the child fetch routes to the right shard.
    const resolver = new MockResolver((database, shard) => {
      if (database.name === "notes") return new MockSqlStore(() => [{ note: `n-${shard[0]}` }]);
      return new MockSqlStore(() => {
        throw new Error("root must not be fetched");
      });
    });
    const plan = selectPlan([
      sqlStep([], "SELECT * FROM root", {
        db: d1("root"),
        mapping: many,
        shard: [["tenant", param("tenantId")]],
      }),
      sqlStep(["notes"], "SELECT * FROM notes", {
        db: doDb("notes"),
        mapping: mapping("Many", [{ parent_key: "tenant", child_key: "tenant" }]),
        shard: [["tenant", spread("tenant")]],
      }),
    ]);

    const body = await executeSelectOk(plan, { tenantId: "A" }, resolver, passthroughWrap, [
      { id: 1 },
      { id: 2 },
    ]);

    expect(resolver.sqlStores.has('notes|["A"]')).toBe(true);
    // Root rows carry the stamped shard field; the child routed to shard A and tagged rows.
    expect(body).toEqual([
      { id: 1, tenant: "A", notes: [{ note: "n-A", tenant: "A" }] },
      { id: 2, tenant: "A", notes: [{ note: "n-A", tenant: "A" }] },
    ]);
  });

  test("a missing join key on seeded rows yields empty children (documented footgun)", async () => {
    // The child spreads `id`, but the seeded root row omits it, so the spread is empty and
    // the child fetch short-circuits to no rows.
    const childStore = new MockSqlStore(() => [{ id: 10, parentId: 1 }]);
    const resolver = new MockResolver((database) =>
      database.name === "children" ? childStore : new MockSqlStore(() => []),
    );
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["children"], 'SELECT * FROM children WHERE "parentId" IN (?1)', {
          db: d1("children"),
          args: [spread("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
    );

    const body = await executeSelectOk(plan, {}, resolver, passthroughWrap, [{ name: "no-id" }]);

    expect(childStore.queries).toEqual([]);
    expect(body).toEqual([{ name: "no-id", children: [] }]);
  });
});

function write(sql: string, args: SaveArg[] = []): SqlStatement {
  return {
    Write: { sql, arguments: args },
  };
}
function hydrate(sql: string, result: PathSegment[], args: SaveArg[] = []): SqlStatement {
  return {
    Hydrate: { sql, arguments: args, result },
  };
}
function payload(v: unknown): SaveArg {
  return { Payload: v };
}
function resultRef(path: PathSegment[]): SaveArg {
  return { Result: path };
}
function field(name: string): PathSegment {
  return { Field: name };
}
function index(i: number): PathSegment {
  return { Index: i };
}

function savePlan(...stages: SaveStep[][]): SavePlan {
  return {
    stages: stages.map((steps) => ({ steps })),
  };
}

function batchStep(
  result: PathSegment[],
  statements: SqlStatement[],
  opts: { db?: Database; shard?: [string, SaveArg][] } = {},
): SaveStep {
  return {
    result,
    query: { SqlBatch: { database: opts.db ?? d1(), shard: opts.shard ?? [], statements } },
  };
}

function keyWriteStep(
  result: PathSegment[],
  database: Database,
  segments: TemplateSegment<SaveArg>[],
  value: unknown,
  metadata: unknown | null = null,
  shard: [string, SaveArg][] = [],
): SaveStep {
  return { result, query: { KeyWrite: { database, segments, value, metadata, shard } } };
}

function saveSynthStep(
  result: PathSegment[],
  fields: [string, SaveArg][],
  create: boolean,
  cardinality: MapCardinality = "One",
): SaveStep {
  return { result, query: { Synthesize: { fields, create, cardinality } } };
}

describe("executeSave SqlBatch", () => {
  test("runs Write then Hydrate and places the read-back row at the result path", async () => {
    const resolver = new MockResolver(
      () =>
        new MockSqlStore(undefined, () => [
          [], // Write returns no rows
          [{ id: 1, name: "a" }], // Hydrate read-back
        ]),
    );
    const plan = savePlan([
      batchStep(
        [],
        [
          write('INSERT INTO "M" ("name") VALUES (?1)', [payload("a")]),
          hydrate('SELECT * FROM "M" WHERE "id" = ?1', [], [payload(1)]),
        ],
      ),
    ]);

    const body = await executeSaveOk(plan, resolver);

    expect(body).toEqual({ id: 1, name: "a" });
    expect(resolver.sqlStores.get("db|[]")!.batches).toEqual([
      {
        statements: [
          { sql: 'INSERT INTO "M" ("name") VALUES (?1)', bindings: ["a"] },
          { sql: 'SELECT * FROM "M" WHERE "id" = ?1', bindings: [1] },
        ],
      },
    ]);
  });

  test("$cloesce_tmp autoincrement capture: hydrate reads back the generated id", async () => {
    // The $cloesce_tmp mechanism lives entirely in the SQL strings; the executor just runs
    // the batch in order. The mock store models a store that captured last_insert_rowid()=42
    // in the tmp table and the hydrate select reads it back.
    let captured: number | null = null;
    const store = new MockSqlStore(undefined, (call) => {
      return call.statements.map((s) => {
        if (s.sql.startsWith("SELECT")) return [{ id: captured, name: "ed" }];
        if (s.sql.startsWith('INSERT INTO "Horse"')) return [];
        if (s.sql.includes("$cloesce_tmp")) {
          captured = 42;
          return [];
        }
        return [];
      });
    });
    const resolver = new MockResolver(() => store);
    const plan = savePlan([
      batchStep(
        [],
        [
          write('INSERT INTO "Horse" ("name") VALUES (?1)', [payload("ed")]),
          write(
            'INSERT OR REPLACE INTO "$cloesce_tmp" ("path", "primary_key") VALUES (\'\', json_object(\'id\', last_insert_rowid()))',
          ),
          hydrate(
            'SELECT "id", "name" FROM "Horse" WHERE "id" = (SELECT json_extract("primary_key", \'$.id\') FROM "$cloesce_tmp" WHERE "path" = \'\')',
            [],
          ),
        ],
      ),
    ]);

    const body = await executeSaveOk(plan, resolver);
    expect(body).toEqual({ id: 42, name: "ed" });
  });

  test("a Hydrate read-back with no row sinks an error", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(undefined, () => [[]]));
    const plan = savePlan([batchStep([], [hydrate("SELECT * FROM M", [])])]);

    const res = await executeSave(plan, resolver);

    expect(res.value).toBeNull();
    expect(res.errors).toEqual([sunkError("generic", /returned no row/)]);
  });

  test("a batch failure sinks, and an independent later step still runs", async () => {
    const kvStore = new MockKeyStore();
    const resolver = new MockResolver(
      () =>
        new MockSqlStore(undefined, () => {
          throw new Error("constraint failed");
        }),
      () => kvStore,
    );
    const plan = savePlan(
      [batchStep([], [write("INSERT INTO M DEFAULT VALUES")])],
      [keyWriteStep([field("blob")], kvDb(), [{ Literal: "k/1" }], { ok: true })],
    );

    const res = await executeSave(plan, resolver);

    // The failure did not halt the plan: the later KV write still landed and its
    // attachment survives in the partial body.
    expect(res.value).toEqual({ blob: { ok: true } });
    expect(res.errors).toEqual([sunkError("generic", /constraint failed/)]);
    expect(kvStore.puts.length).toBe(1);
  });
});

describe("executeSave DO SqlBatch shard tagging", () => {
  test("routes to the shard stub and tags hydrated rows with route fields", async () => {
    const resolver = new MockResolver(
      () => new MockSqlStore(undefined, () => [[], [{ id: 1, name: "n" }]]),
    );
    const plan = savePlan([
      batchStep(
        [],
        [
          write('INSERT INTO "M" ("name") VALUES (?1)', [payload("n")]),
          hydrate('SELECT * FROM "M" WHERE "id" = ?1', [], [payload(1)]),
        ],
        { db: doDb("agg"), shard: [["tenant", payload("A")]] },
      ),
    ]);

    const body = await executeSaveOk(plan, resolver);

    expect(body).toEqual({ id: 1, name: "n", tenant: "A" });
    expect(resolver.sqlStores.has('agg|["A"]')).toBe(true);
  });
});

describe("executeSave KeyWrite", () => {
  test("KV write records key, value, and metadata and attaches value at the result path", async () => {
    const resolver = new MockResolver();
    const plan = savePlan(
      [saveSynthStep([], [["id", payload(7)]], true)],
      [
        keyWriteStep(
          [field("profile")],
          kvDb(),
          [{ Literal: "profile:" }, { Value: resultRef([field("id")]) }],
          { bio: "hi" },
          { v: 3 },
        ),
      ],
    );

    const body = await executeSaveOk(plan, resolver);

    expect(body).toEqual({ id: 7, profile: { bio: "hi" } });
    expect(resolver.keyStores.get("kv|[]")!.puts).toEqual([
      { key: "profile:7", value: { bio: "hi" }, metadata: { v: 3 } },
    ]);
  });

  test("R2-style write passes undefined metadata", async () => {
    const resolver = new MockResolver();
    const plan = savePlan([
      keyWriteStep([field("blob")], r2Db(), [{ Literal: "blob:1" }], { bytes: [1, 2] }),
    ]);

    const body = await executeSaveOk(plan, resolver);

    expect(body).toEqual({ blob: { bytes: [1, 2] } });
    expect(resolver.keyStores.get("r2|[]")!.puts).toEqual([
      { key: "blob:1", value: { bytes: [1, 2] }, metadata: undefined },
    ]);
  });

  test("DO-KV write routes by its shard tuple", async () => {
    const resolver = new MockResolver();
    const plan = savePlan([
      keyWriteStep([field("entry")], doDb("dokv"), [{ Literal: "entry:1" }], { n: 1 }, null, [
        ["tenant", payload("B")],
      ]),
    ]);

    await executeSaveOk(plan, resolver);

    expect(resolver.keyStores.has('dokv|["B"]')).toBe(true);
    expect(resolver.keyStores.get('dokv|["B"]')!.puts[0].key).toBe("entry:1");
  });
});

describe("executeSave nested PathSegment attach", () => {
  test("writes hydrated rows into nested arrays and objects", async () => {
    const resolver = new MockResolver((database) =>
      database.name === "root"
        ? new MockSqlStore(undefined, () => [[], [{ id: 1 }]])
        : new MockSqlStore(undefined, () => [[], [{ id: 10 }]]),
    );
    const plan = savePlan(
      [
        batchStep(
          [],
          [write("INSERT INTO root DEFAULT VALUES"), hydrate("SELECT * FROM root", [])],
          { db: d1("root") },
        ),
      ],
      [
        batchStep(
          [field("children"), index(0)],
          [
            write("INSERT INTO children DEFAULT VALUES"),
            hydrate("SELECT * FROM children", [field("children"), index(0)]),
          ],
          { db: d1("children") },
        ),
      ],
    );

    const body = await executeSaveOk(plan, resolver);
    expect(body).toEqual({ id: 1, children: [{ id: 10 }] });
  });
});

describe("executeSave arg resolution", () => {
  test("Payload literals and Result path-references both bind into statements", async () => {
    const resolver = new MockResolver(
      () =>
        new MockSqlStore(undefined, () => [
          [], // synthesize-less: first write
          [{ dogId: 3, id: 9 }], // hydrate
        ]),
    );
    const plan = savePlan(
      // First stage: hydrate a "dog" body at Field("dog") holding an id.
      [saveSynthStep([field("dog")], [["id", payload(3)]], true)],
      // Second stage: a write binding a Payload literal and a Result reference to dog.id.
      [
        batchStep(
          [],
          [
            write('INSERT INTO "Person" ("dogId", "id") VALUES (?1, ?2)', [
              resultRef([field("dog"), field("id")]),
              payload(9),
            ]),
            hydrate('SELECT * FROM "Person" WHERE "id" = ?1', [], [payload(9)]),
          ],
        ),
      ],
    );

    const body = await executeSaveOk(plan, resolver);

    expect(resolver.sqlStores.get("db|[]")!.batches[0].statements[0]).toEqual({
      sql: 'INSERT INTO "Person" ("dogId", "id") VALUES (?1, ?2)',
      bindings: [3, 9], // dog.id resolved from body, then the literal payload
    });
    expect(body.dog).toEqual({ id: 3 });
  });

  test("a Result reference to a missing body value sinks an error", async () => {
    const resolver = new MockResolver();
    const plan = savePlan([
      batchStep([], [write("INSERT INTO M (x) VALUES (?1)", [resultRef([field("nope")])])]),
    ]);

    const res = await executeSave(plan, resolver);

    expect(res.value).toBeNull();
    expect(res.errors).toEqual([sunkError("generic", /missing hydrated value/)]);
  });
});

describe("executeSave Synthesize merge", () => {
  test("create=false merges fields into an existing body object", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(undefined, () => [[], [{ id: 1 }]]));
    const plan = savePlan(
      [batchStep([], [write("INSERT INTO M DEFAULT VALUES"), hydrate("SELECT * FROM M", [])])],
      [saveSynthStep([], [["extra", payload("v")]], false)],
    );

    const body = await executeSaveOk(plan, resolver);
    expect(body).toEqual({ id: 1, extra: "v" });
  });

  test("create=true with Many cardinality and no fields attaches an empty array", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(undefined, () => [[], [{ id: 1 }]]));
    const plan = savePlan([
      batchStep([], [write("INSERT INTO M DEFAULT VALUES"), hydrate("SELECT * FROM M", [])]),
      saveSynthStep([field("children")], [], true, "Many"),
    ]);

    const body = await executeSaveOk(plan, resolver);
    expect(body).toEqual({ id: 1, children: [] });
  });
});
