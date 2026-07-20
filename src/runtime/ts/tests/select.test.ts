import { describe, test, expect } from "vitest";
import {
  executeSelect,
  MAX_BOUND_PARAMETERS,
  type KeyStore,
  type SqlStore,
} from "../src/router/executor/index.js";
import type { Mapping } from "../src/router/executor/plan.js";
import { KValue } from "../src/ui/backend.js";
import {
  MockKeyStore,
  MockResolver,
  MockSqlStore,
  d1,
  doDb,
  kvDb,
  r2Db,
  sunkError,
} from "./common/executor.js";
import {
  executeSelectOk,
  keyStep,
  many,
  mapping,
  param,
  scalarField,
  selectPlan,
  spread,
  sqlStep,
  synthStep,
  tuple,
  type RawArg,
} from "./common/select.js";

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

    const body = await executeSelectOk(plan, { id: 1 }, resolver);

    expect(body).toEqual({ id: 1, name: "a" });
    expect(resolver.sqlStores.get("db|[]")!.queries).toEqual([
      { sql: 'SELECT * FROM "M" WHERE "id" = ?', bindings: [1] },
    ]);
  });

  test("One cardinality yields null when no rows come back", async () => {
    const plan = selectPlan([sqlStep([], "SELECT 1")]);
    const body = await executeSelectOk(plan, {}, new MockResolver());
    expect(body).toBeNull();
  });

  test("Many cardinality maps all rows to a root array", async () => {
    const plan = selectPlan([sqlStep([], "SELECT * FROM M", { mapping: many })]);
    const rows = [{ id: 1 }, { id: 2 }];
    const resolver = new MockResolver(() => new MockSqlStore(() => rows));

    const body = await executeSelectOk(plan, {}, resolver);
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
      [
        sqlStep([], "SELECT * FROM parents", {
          db: d1("parents"),
          mapping: many,
        }),
      ],
      [
        sqlStep(["children"], 'SELECT * FROM children WHERE "parentId" IN (?1)', {
          db: d1("children"),
          args: [spread("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
    );

    const body = await executeSelectOk(plan, {}, resolver);

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

    const body = await executeSelectOk(plan, {}, resolver);
    expect(body).toEqual({
      id: 1,
      ownerId: 7,
      owner: { id: 7, name: "owner" },
    });
  });
});

describe("executeSelect join bucketing", () => {
  /** A Many root over `parents`, then a child stage joining on `join` with `child` rows. */
  const joinPlan = (join: Mapping["join"]) =>
    selectPlan(
      [sqlStep([], "SELECT * FROM parents", { db: d1("parents"), mapping: many })],
      [
        sqlStep(["children"], 'SELECT * FROM children WHERE "pid" IN (?1)', {
          db: d1("children"),
          args: [spread("id")],
          mapping: mapping("Many", join),
        }),
      ],
    );

  const runJoin = (join: Mapping["join"], parents: any[], children: any[]) => {
    const resolver = new MockResolver((database) =>
      database.name === "parents"
        ? new MockSqlStore(() => parents)
        : new MockSqlStore(() => children),
    );
    return executeSelectOk(joinPlan(join), {}, resolver);
  };

  test("single-key join distributes rows identically to the multi-key path, including nulls", async () => {
    // A null parent key matches a null child key (existing `?? null` joinKey semantics); a
    // real key matches only its own. Parent 2's key is null, so it collects the null child.
    const parents = [
      { id: 1, k: "a", g: "x" },
      { id: 2, k: null, g: "x" },
      { id: 3, k: "b", g: "x" },
    ];
    const children = [
      { c: 10, pid: "a", g: "x" },
      { c: 11, pid: null, g: "x" },
      { c: 12, pid: "b", g: "x" },
      { c: 13, pid: "a", g: "x" },
    ];

    // The single-key fast path and a two-key join whose extra pair always matches (`g`)
    // must distribute the rows the same way.
    const single = await runJoin([{ parent_key: "k", child_key: "pid" }], parents, children);
    const multi = await runJoin(
      [
        { parent_key: "k", child_key: "pid" },
        { parent_key: "g", child_key: "g" },
      ],
      parents,
      children,
    );

    const expected = [
      {
        id: 1,
        k: "a",
        g: "x",
        children: [
          { c: 10, pid: "a", g: "x" },
          { c: 13, pid: "a", g: "x" },
        ],
      },
      { id: 2, k: null, g: "x", children: [{ c: 11, pid: null, g: "x" }] },
      { id: 3, k: "b", g: "x", children: [{ c: 12, pid: "b", g: "x" }] },
    ];
    expect(single).toEqual(expected);
    expect(multi).toEqual(single);
  });

  test("one fetched row matching two parents yields independent objects per parent", async () => {
    // Two parents share the single join key "a"; one child row matches both. The first parent
    // gets the fetched object itself, the second a clone — so mutating one must not touch the
    // other.
    const parents = [
      { id: 1, k: "a" },
      { id: 2, k: "a" },
    ];
    const body = await runJoin([{ parent_key: "k", child_key: "pid" }], parents, [
      { c: 10, pid: "a" },
    ]);

    expect(body[0].children[0]).toEqual({ c: 10, pid: "a" });
    expect(body[1].children[0]).toEqual({ c: 10, pid: "a" });
    expect(body[0].children[0]).not.toBe(body[1].children[0]);

    body[0].children[0].c = 999;
    expect(body[1].children[0].c).toBe(10);
  });

  test("a single parent match attaches the fetched row's data", async () => {
    const body = await runJoin(
      [{ parent_key: "k", child_key: "pid" }],
      [{ id: 1, k: "a" }],
      [{ c: 10, pid: "a", extra: "kept" }],
    );
    expect(body).toEqual([{ id: 1, k: "a", children: [{ c: 10, pid: "a", extra: "kept" }] }]);
  });
});

describe("executeSelect scalar (non-spread) SQL argument", () => {
  /** A Many root over `parents`, then a child binding `id` as a scalar (non-spread) arg. */
  const scalarPlan = () =>
    selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["children"], 'SELECT * FROM children WHERE "parentId" = ?1', {
          db: d1("children"),
          args: [scalarField("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
        }),
      ],
    );

  test("a non-spread field resolving to one distinct value binds it", async () => {
    // Every parent shares the same id, so the scalar arg has exactly one value to bind.
    const parents = [{ id: 1 }, { id: 1 }];
    const childStore = new MockSqlStore(() => []);
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : childStore,
    );

    await executeSelectOk(scalarPlan(), {}, resolver);

    expect(childStore.queries).toEqual([
      { sql: 'SELECT * FROM children WHERE "parentId" = ?', bindings: [1] },
    ]);
  });

  test("a non-spread field resolving to multiple distinct values fails the step", async () => {
    // Three distinct parent ids can't collapse to a single scalar bind, so the step errors.
    const parents = [{ id: 1 }, { id: 2 }, { id: 3 }];
    const childStore = new MockSqlStore(() => []);
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : childStore,
    );

    const res = await executeSelect(scalarPlan(), {}, resolver);

    // The Many child degrades to [] on each parent and the error is sunk; no query ran.
    expect(res.value).toEqual([
      { id: 1, children: [] },
      { id: 2, children: [] },
      { id: 3, children: [] },
    ]);
    expect(res.errors).toEqual([
      sunkError("generic", /non-spread argument resolving to 3 distinct values/),
    ]);
    expect(childStore.queries).toEqual([]);
  });
});

describe("executeSelect bulk key reads", () => {
  /** A {@link KeyStore} exposing `getMany`, recording both bulk and per-key reads. */
  class MockBulkKeyStore implements KeyStore {
    getManyCalls: string[][] = [];
    gets: string[] = [];

    constructor(private store: Map<string, unknown> = new Map()) {}

    get(key: string): unknown {
      this.gets.push(key);
      return this.store.has(key) ? this.store.get(key) : null;
    }

    async getMany(keys: string[]): Promise<Map<string, unknown>> {
      this.getManyCalls.push(keys);
      return new Map(keys.map((k) => [k, this.store.has(k) ? this.store.get(k) : null]));
    }

    put(): void {}
  }

  test("prefers getMany, calling it once with every key instead of per-key get", async () => {
    const bulk = new MockBulkKeyStore(
      new Map<string, unknown>([
        ["k/1", { value: "A" }],
        ["k/2", { value: "B" }],
      ]),
    );
    const resolver = new MockResolver(
      () => new MockSqlStore(() => [{ id: 1 }, { id: 2 }]),
      () => bulk as unknown as MockKeyStore,
    );
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM M", { mapping: many })],
      [keyStep(["data"], kvDb("kv"), [{ Literal: "k/" }, { Value: spread("id") }])],
    );

    const body = await executeSelectOk(plan, {}, resolver);

    expect(bulk.getManyCalls).toEqual([["k/1", "k/2"]]);
    expect(bulk.gets).toEqual([]);
    expect(body).toEqual([
      { id: 1, data: new KValue("A") },
      { id: 2, data: new KValue("B") },
    ]);
  });
});

describe("executeSelect spread IN expansion", () => {
  /** A root list stage plus one child stage joining `children.parentId -> root.id`. */
  const childPlan = (childArgs: RawArg[], childSql: string) =>
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
    );

    expect(resolver.sqlStores.get("children|[]")!.queries).toEqual([
      {
        sql: 'SELECT * FROM children WHERE "parentId" IN (?, ?, ?)',
        bindings: [1, 2, 3],
      },
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
    );

    expect(body).toEqual([{ id: 1, children: [] }]);
    expect(childStore.queries).toEqual([]);
  });

  test("expands ?1 vs ?10 without a naive string-replace collision", async () => {
    // Ten single-value params: only ?1 and ?10 differ by a prefix; a naive replace of `?1`
    // would corrupt `?10`. `IN (?1)` must stay one placeholder and `?10` its own.
    const args: RawArg[] = Array.from({ length: 10 }, (_, i) => param(`p${i + 1}`));
    const params = Object.fromEntries(args.map((_, i) => [`p${i + 1}`, i + 1]));
    const store = new MockSqlStore(() => [{ ok: 1 }]);
    const resolver = new MockResolver(() => store);
    const plan = selectPlan([
      sqlStep([], "SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10", { args }),
    ]);

    await executeSelectOk(plan, params, resolver);

    expect(store.queries).toEqual([
      {
        sql: "SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?",
        bindings: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
      },
    ]);
  });
});

describe("executeSelect spread chunking (MAX_BOUND_PARAMETERS)", () => {
  /** Build a two-stage plan whose child SQL spreads `id` over `?1`, plus optional fixed params. */
  const chunkPlan = (childArgs: RawArg[], childSql: string) =>
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
    const parents = Array.from({ length: MAX_BOUND_PARAMETERS }, (_, i) => ({
      id: i + 1,
    }));
    const childStore = new MockSqlStore(() => []);
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : childStore,
    );

    await executeSelectOk(
      chunkPlan([spread("id")], 'SELECT * FROM children WHERE "parentId" IN (?1)'),
      {},
      resolver,
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
          s.bindings.map((pid) => ({
            id: 1000 + (pid as number),
            parentId: pid,
          })),
        ),
    );
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : childStore,
    );

    const body = await executeSelectOk(
      chunkPlan([spread("id")], 'SELECT * FROM children WHERE "parentId" IN (?1)'),
      {},
      resolver,
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
    const parents = Array.from({ length: n }, (_, i) => ({
      id: i + 1,
      tag: i + 1,
    }));
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

describe("executeSelect composite tuple-spread IN", () => {
  /** A Many root plus a child whose nav spreads a (a, b) row-value tuple. */
  const tuplePlan = (childSql: string) =>
    selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["children"], childSql, {
          db: d1("children"),
          args: [tuple(spread("a"), spread("b"))],
          mapping: mapping("Many", [
            { parent_key: "a", child_key: "a" },
            { parent_key: "b", child_key: "b" },
          ]),
        }),
      ],
    );

  test("binds N deduped tuples, not the N x M cross product of the columns", async () => {
    // Three parents over two distinct (a, b) pairs: (1,10), (2,20), (1,10) again.
    // Per-column dedupe would yield a={1,2} x b={10,20} = 4 rows (cross product);
    // the row-value form must bind exactly the 2 real pairs.
    const parents = [
      { a: 1, b: 10 },
      { a: 2, b: 20 },
      { a: 1, b: 10 },
    ];
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : new MockSqlStore(() => []),
    );

    await executeSelectOk(
      tuplePlan('SELECT * FROM children WHERE ("a", "b") IN (VALUES ?1)'),
      {},
      resolver,
    );

    expect(resolver.sqlStores.get("children|[]")!.queries).toEqual([
      {
        sql: 'SELECT * FROM children WHERE ("a", "b") IN (VALUES (?, ?), (?, ?))',
        bindings: [1, 10, 2, 20],
      },
    ]);
  });

  test("width-2 tuples chunk at 50 elements per statement near the 100-param cap", async () => {
    // 60 distinct (a, b) pairs -> 120 params -> chunks of 50 + 10 tuples (100 + 20 params).
    const n = 60;
    const parents = Array.from({ length: n }, (_, i) => ({ a: i + 1, b: 1000 + i }));
    const childStore = new MockSqlStore(
      () => [],
      (call) => call.statements.map(() => []),
    );
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : childStore,
    );

    await executeSelectOk(
      tuplePlan('SELECT * FROM children WHERE ("a", "b") IN (VALUES ?1)'),
      {},
      resolver,
    );

    const stmts = childStore.batches[0].statements;
    expect(stmts.length).toBe(2); // ceil(60 / 50)
    // Each statement's bindings are whole (a, b) pairs and stay within the cap.
    for (const s of stmts) {
      expect(s.bindings.length % 2).toBe(0);
      expect(s.bindings.length).toBeLessThanOrEqual(MAX_BOUND_PARAMETERS);
    }
    expect(stmts[0].bindings.length).toBe(100); // 50 tuples x 2
    expect(stmts[1].bindings.length).toBe(20); // 10 tuples x 2
    // A tuple's two values stay adjacent and the union covers every pair exactly once.
    const pairs = new Set<string>();
    for (const s of stmts) {
      for (let i = 0; i < s.bindings.length; i += 2) {
        pairs.add(JSON.stringify([s.bindings[i], s.bindings[i + 1]]));
      }
    }
    expect(pairs.size).toBe(n);
    expect(parents.every((p) => pairs.has(JSON.stringify([p.a, p.b])))).toBe(true);
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

    const body = await executeSelectOk(plan, {}, resolver);

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

describe("executeSelect DO shard-correlated spreads", () => {
  /** A Many root over `parents`, then a DO nav sharded by `tenant` and spreading `id`. */
  const shardedSpreadPlan = () =>
    selectPlan(
      [sqlStep([], "SELECT * FROM root", { db: d1("root"), mapping: many })],
      [
        sqlStep(["notes"], 'SELECT * FROM notes WHERE "ownerId" IN (?1)', {
          db: doDb("notes"),
          args: [spread("id")],
          mapping: mapping("Many", [{ parent_key: "id", child_key: "ownerId" }]),
          shard: [["tenant", spread("tenant")]],
        }),
      ],
    );

  test("each shard binds only its own distinct spread values, not the global set", async () => {
    const parents = [
      { id: 1, tenant: "A" },
      { id: 2, tenant: "A" },
      { id: 3, tenant: "B" },
    ];
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : new MockSqlStore(() => []),
    );

    await executeSelectOk(shardedSpreadPlan(), {}, resolver);

    // Shard A queries only ids 1,2; shard B only id 3 — never the global [1,2,3].
    expect(resolver.sqlStores.get('notes|["A"]')!.queries).toEqual([
      { sql: 'SELECT * FROM notes WHERE "ownerId" IN (?, ?)', bindings: [1, 2] },
    ]);
    expect(resolver.sqlStores.get('notes|["B"]')!.queries).toEqual([
      { sql: 'SELECT * FROM notes WHERE "ownerId" IN (?)', bindings: [3] },
    ]);
  });

  test("chunking is scoped per shard: a local count under the limit is never chunked", async () => {
    // Two shards of 60 ids each — 120 global would chunk, but 60 < limit per shard.
    const parents = [
      ...Array.from({ length: 60 }, (_, i) => ({ id: i + 1, tenant: "A" })),
      ...Array.from({ length: 60 }, (_, i) => ({ id: 100 + i + 1, tenant: "B" })),
    ];
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : new MockSqlStore(() => []),
    );

    await executeSelectOk(shardedSpreadPlan(), {}, resolver);

    for (const tenant of ["A", "B"]) {
      const store = resolver.sqlStores.get(`notes|["${tenant}"]`)!;
      expect(store.queries.length).toBe(1);
      expect(store.batches.length).toBe(0);
      expect(store.queries[0].bindings.length).toBe(60);
    }
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

    const body = await executeSelectOk(plan, {}, resolver);

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

    const body = await executeSelectOk(plan, {}, resolver);
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

    const body = await executeSelectOk(plan, { id: 5, name: "z" }, new MockResolver());
    expect(body).toEqual({ id: 5, name: "z" });
  });

  test("merges synthesized fields onto the buffer an earlier step already produced at the path", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(() => [{ id: 1 }, { id: 2 }]));
    const plan = selectPlan(
      [sqlStep([], "SELECT * FROM M", { mapping: many })],
      [synthStep([], [["tag", param("tag")]], "Many")],
    );

    const body = await executeSelectOk(plan, { tag: "t" }, resolver);
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

    const body = await executeSelectOk(plan, { label: "L" }, resolver);
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

    const body = await executeSelectOk(plan, { role: "admin" }, resolver);
    expect(body).toEqual({
      id: 1,
      ownerId: 7,
      owner: { id: 7, name: "o", role: "admin" },
    });
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

    const body = await executeSelectOk(plan, {}, resolver);
    expect(body).toEqual({
      id: 1,
      b: { id: 10, aId: 1, c: { id: 100, bId: 10 } },
    });
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

    await executeSelectOk(plan, { lastSeen_id: 5, limit: 20 }, resolver);

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

    const resultPromise = executeSelectOk(plan, {}, resolver);

    for (let i = 0; i < 10 && maxConcurrent.count < 2; i++) {
      await Promise.resolve();
    }
    expect(maxConcurrent.count).toBe(2); // both parents' reads were in flight simultaneously

    deferredByKey.get("k/1")!.resolve({ value: "A" });
    deferredByKey.get("k/2")!.resolve({ value: "B" });
    const body = await resultPromise;
    expect(body).toEqual([
      { id: 1, data: new KValue("A") },
      { id: 2, data: new KValue("B") },
    ]);
  });

  test("a Key step in the same stage as its root Sql step sees the attached row", async () => {
    // The GET-with-KV shape: a root Sql step and a Key step keyed off a param, in ONE stage.
    // The Key attaches under the root object, which the Sql step produces earlier in the same
    // stage — so the step-ordered sink must run the Sql attach before the Key read/attach.
    const resolver = new MockResolver(
      () => new MockSqlStore(() => [{ id: 7 }]),
      () => new MockKeyStore(new Map([["k/7", { value: "V" }]])),
    );
    const plan = selectPlan([
      sqlStep([], "SELECT * FROM M WHERE id = ?1", { args: [param("id")] }),
      keyStep(["data"], kvDb("kv"), [{ Literal: "k/" }, { Value: param("id") }]),
    ]);

    const body = await executeSelectOk(plan, { id: 7 }, resolver);
    expect(body).toEqual({ id: 7, data: new KValue("V") });
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

    const resultPromise = executeSelectOk(plan, {}, resolver);

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

    const res = await executeSelect(plan, {}, new MockResolver());

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

    const res = await executeSelect(plan, {}, resolver);

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

    const body = await executeSelectOk(plan, {}, resolver, [
      {
        id: 1,
        children: [
          { id: 10, parentId: 1 },
          { id: 11, parentId: 1 },
        ],
      },
    ]);

    expect(grandStore.queries).toEqual([
      {
        sql: 'SELECT * FROM notes WHERE "childId" IN (?, ?)',
        bindings: [10, 11],
      },
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

    const body = await executeSelectOk(plan, {}, resolver, [{ id: 1, children: [] }]);

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

    const res = await executeSelect(plan, {}, new MockResolver(), [
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

    const body = await executeSelectOk(plan, {}, resolver, [{ id: 1, ownerId: 7, owner: null }]);

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

    const body = await executeSelectOk(plan, { tenantId: "A" }, resolver, [{ id: 1 }, { id: 2 }]);

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

    const body = await executeSelectOk(plan, {}, resolver, [{ name: "no-id" }]);

    expect(childStore.queries).toEqual([]);
    expect(body).toEqual([{ name: "no-id", children: [] }]);
  });
});
