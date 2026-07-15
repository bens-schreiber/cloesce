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
  Mapping,
  PathSegment,
  SaveArg,
  SavePlan,
  SelectArg,
  SelectPlan,
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
 * A fake {@link StorageResolver}. `sql`/`key` are resolved by a `(database, shard) -> store`
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

// ----------------------------------------------------------------------------
// Plan-literal factory helpers
// ----------------------------------------------------------------------------

const d1 = (name = "db"): Database => ({ name, kind: "D1" });
const doDb = (name = "doDb"): Database => ({ name, kind: "DurableObject" });
const kvDb = (name = "kv"): Database => ({ name, kind: "Kv" });
const r2Db = (name = "r2"): Database => ({ name, kind: "R2" });

const one: Mapping = { cardinality: "One", join: [] };
const many: Mapping = { cardinality: "Many", join: [] };
const mapping = (cardinality: "One" | "Many", join: Mapping["join"] = []): Mapping => ({
  cardinality,
  join,
});

// A Sql `arguments` slot is a SelectArg: a `Param` binds one value, a `ParentField` spreads
// the distinct values of that field across the parents into an `IN (...)` list.
const param = (name: string): SelectArg => ({ Param: name });
const spread = (field: string): SelectArg => ({ ParentField: field });

const passthroughWrap: KeyValueWrapper = (_db, _path, raw) => raw ?? null;
const kValueWrap: KeyValueWrapper = (database, _path, raw, metadata) => {
  if (database.kind === "Kv") {
    const inner = (raw ?? {}) as { value?: unknown; metadata?: unknown };
    return new KValue(inner.value ?? null, inner.metadata ?? metadata ?? null);
  }
  return raw ?? null;
};

// ----------------------------------------------------------------------------
// executeSelect
// ----------------------------------------------------------------------------

describe("executeSelect cardinality", () => {
  test("One cardinality maps the first row to a root object", async () => {
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: 'SELECT * FROM "M" WHERE "id" = ?1',
                  arguments: [param("id")],
                  mapping: one,
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };
    const resolver = new MockResolver(
      () =>
        new MockSqlStore(() => [
          { id: 1, name: "a" },
          { id: 1, name: "b" },
        ]),
    );

    const body = await executeSelect(plan, { id: 1 }, resolver, passthroughWrap);

    expect(body).toEqual({ id: 1, name: "a" });
    expect(resolver.sqlStores.get("db|[]")!.queries).toEqual([
      { sql: 'SELECT * FROM "M" WHERE "id" = ?', bindings: [1] },
    ]);
  });

  test("One cardinality yields null when no rows come back", async () => {
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: "SELECT 1",
                  arguments: [],
                  mapping: one,
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };
    const body = await executeSelect(plan, {}, new MockResolver(), passthroughWrap);
    expect(body).toBeNull();
  });

  test("Many cardinality maps all rows to a root array", async () => {
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: "SELECT * FROM M",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };
    const rows = [{ id: 1 }, { id: 2 }];
    const resolver = new MockResolver(() => new MockSqlStore(() => rows));

    const body = await executeSelect(plan, {}, resolver, passthroughWrap);
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
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("parents"),
                  sql: "SELECT * FROM parents",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["children"],
              query: {
                Sql: {
                  database: d1("children"),
                  sql: 'SELECT * FROM children WHERE "parentId" IN (?1)',
                  arguments: [spread("id")],
                  mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, {}, resolver, passthroughWrap);

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
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("root"),
                  sql: "SELECT * FROM root",
                  arguments: [],
                  mapping: one,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["owner"],
              query: {
                Sql: {
                  database: d1("owners"),
                  sql: 'SELECT * FROM owners WHERE "id" IN (?1)',
                  arguments: [spread("ownerId")],
                  mapping: mapping("One", [{ parent_key: "ownerId", child_key: "id" }]),
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, {}, resolver, passthroughWrap);
    expect(body).toEqual({ id: 1, ownerId: 7, owner: { id: 7, name: "owner" } });
  });
});

describe("executeSelect spread IN expansion", () => {
  test("expands a Spread into one placeholder per deduped value", async () => {
    const parents = [{ id: 1 }, { id: 2 }, { id: 2 }, { id: 3 }];
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : new MockSqlStore(() => []),
    );
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("root"),
                  sql: "SELECT * FROM root",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["children"],
              query: {
                Sql: {
                  database: d1("children"),
                  sql: 'SELECT * FROM children WHERE "parentId" IN (?1)',
                  arguments: [spread("id")],
                  mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    await executeSelect(plan, {}, resolver, passthroughWrap);

    expect(resolver.sqlStores.get("children|[]")!.queries).toEqual([
      { sql: 'SELECT * FROM children WHERE "parentId" IN (?, ?, ?)', bindings: [1, 2, 3] },
    ]);
  });

  test("renumbers correctly when a Param arg follows the Spread", async () => {
    const parents = [{ id: 1 }, { id: 2 }];
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : new MockSqlStore(() => []),
    );
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("root"),
                  sql: "SELECT * FROM root",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["children"],
              query: {
                Sql: {
                  database: d1("children"),
                  sql: 'SELECT * FROM children WHERE "parentId" IN (?1) AND "kind" = ?2',
                  arguments: [spread("id"), param("kind")],
                  mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    await executeSelect(plan, { kind: "active" }, resolver, passthroughWrap);

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
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("root"),
                  sql: "SELECT * FROM root",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["children"],
              query: {
                Sql: {
                  database: d1("children"),
                  sql: 'SELECT * FROM children WHERE "parentId" IN (?1)',
                  // `missing` is absent from every parent, so the spread is empty.
                  arguments: [spread("missing")],
                  mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, {}, resolver, passthroughWrap);

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
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: "SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10",
                  arguments: args,
                  mapping: one,
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    await executeSelect(plan, params, resolver, passthroughWrap);

    expect(store.queries).toEqual([
      { sql: "SELECT ?, ?, ?, ?, ?, ?, ?, ?, ?, ?", bindings: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] },
    ]);
  });
});

describe("executeSelect spread chunking (MAX_BOUND_PARAMETERS)", () => {
  /** Build a two-stage plan whose child SQL spreads `id` over `?1`, plus optional fixed params. */
  function chunkPlan(childArgs: SelectArg[], childSql: string): SelectPlan {
    return {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("root"),
                  sql: "SELECT * FROM root",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["children"],
              query: {
                Sql: {
                  database: d1("children"),
                  sql: childSql,
                  arguments: childArgs,
                  mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };
  }

  test("exactly at the limit issues a single query and no batch", async () => {
    const parents = Array.from({ length: MAX_BOUND_PARAMETERS }, (_, i) => ({ id: i + 1 }));
    const childStore = new MockSqlStore(() => []);
    const resolver = new MockResolver((database) =>
      database.name === "root" ? new MockSqlStore(() => parents) : childStore,
    );

    await executeSelect(
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

    const body = await executeSelect(
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
    await executeSelect(
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
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("root"),
                  sql: "SELECT * FROM root",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["children"],
              query: {
                Sql: {
                  database: d1("children"),
                  sql: 'SELECT * FROM children WHERE "parentId" IN (?1) AND "tagId" IN (?2)',
                  arguments: [spread("id"), spread("tag")],
                  mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    await executeSelect(plan, {}, resolver, passthroughWrap);

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
    const shardArg: [string, SelectArg][] = [["tenant", spread("tenant")]];
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("root"),
                  sql: "SELECT * FROM root",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["notes"],
              query: {
                Sql: {
                  database: doDb("notes"),
                  sql: "SELECT * FROM notes",
                  arguments: [],
                  mapping: mapping("Many", [{ parent_key: "tenant", child_key: "tenant" }]),
                  shard: shardArg,
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, {}, resolver, passthroughWrap);

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
    const key: TemplateSegment<SelectArg>[] = [
      { Literal: "profile:" },
      { Value: { ParentField: "id" } },
    ];
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: "SELECT * FROM M",
                  arguments: [],
                  mapping: one,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["profile"],
              query: { Key: { database: kvDb(), segments: key, shard: [] } },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, {}, resolver, kValueWrap);

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
    const key: TemplateSegment<SelectArg>[] = [
      { Literal: "blob:" },
      { Value: { ParentField: "id" } },
    ];
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: "SELECT * FROM M",
                  arguments: [],
                  mapping: one,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["blob"],
              query: { Key: { database: r2Db(), segments: key, shard: [] } },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, {}, resolver, passthroughWrap);
    expect(body.blob).toEqual({ bytes: [1, 2, 3] });
  });
});

describe("executeSelect Synthesize steps", () => {
  test("materializes a root object from params when no buffer exists at the path yet", async () => {
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Synthesize: {
                  fields: [
                    ["id", { Param: "id" }],
                    ["name", { Param: "name" }],
                  ],
                  cardinality: "One",
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(
      plan,
      { id: 5, name: "z" },
      new MockResolver(),
      passthroughWrap,
    );
    expect(body).toEqual({ id: 5, name: "z" });
  });

  test("merges synthesized fields onto the buffer an earlier step already produced at the path", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(() => [{ id: 1 }, { id: 2 }]));
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: "SELECT * FROM M",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: [],
              query: {
                Synthesize: {
                  fields: [["tag", { Param: "tag" }]],
                  cardinality: "Many",
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, { tag: "t" }, resolver, passthroughWrap);
    expect(body).toEqual([
      { id: 1, tag: "t" },
      { id: 2, tag: "t" },
    ]);
  });

  test("materializes a child object under a parent from params and parent fields", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(() => [{ id: 1, ownerId: 7 }]));
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: "SELECT * FROM M",
                  arguments: [],
                  mapping: one,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["owner"],
              query: {
                Synthesize: {
                  fields: [
                    ["id", { ParentField: "ownerId" }],
                    ["label", { Param: "label" }],
                  ],
                  cardinality: "One",
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, { label: "L" }, resolver, passthroughWrap);
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
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("root"),
                  sql: "SELECT * FROM root",
                  arguments: [],
                  mapping: one,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["owner"],
              query: {
                Sql: {
                  database: d1("owners"),
                  sql: 'SELECT * FROM owners WHERE "id" IN (?1)',
                  arguments: [spread("ownerId")],
                  mapping: mapping("One", [{ parent_key: "ownerId", child_key: "id" }]),
                  shard: [],
                },
              },
            },
            // Same `owner` path: merges `role` onto the existing owner buffer.
            {
              result: ["owner"],
              query: {
                Synthesize: {
                  fields: [["role", { Param: "role" }]],
                  cardinality: "One",
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, { role: "admin" }, resolver, passthroughWrap);
    expect(body).toEqual({ id: 1, ownerId: 7, owner: { id: 7, name: "o", role: "admin" } });
  });

  test("assembles deepest paths first: a grandchild lands inside its child before the child attaches", async () => {
    const resolver = new MockResolver((database) => {
      if (database.name === "a") return new MockSqlStore(() => [{ id: 1 }]);
      if (database.name === "b") return new MockSqlStore(() => [{ id: 10, aId: 1 }]);
      return new MockSqlStore(() => [{ id: 100, bId: 10 }]);
    });
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("a"),
                  sql: "SELECT * FROM a",
                  arguments: [],
                  mapping: one,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["b"],
              query: {
                Sql: {
                  database: d1("b"),
                  sql: 'SELECT * FROM b WHERE "aId" IN (?1)',
                  arguments: [spread("id")],
                  mapping: mapping("One", [{ parent_key: "id", child_key: "aId" }]),
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["b", "c"],
              query: {
                Sql: {
                  database: d1("c"),
                  sql: 'SELECT * FROM c WHERE "bId" IN (?1)',
                  arguments: [spread("id")],
                  mapping: mapping("One", [{ parent_key: "id", child_key: "bId" }]),
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, {}, resolver, passthroughWrap);
    expect(body).toEqual({ id: 1, b: { id: 10, aId: 1, c: { id: 100, bId: 10 } } });
  });
});

describe("executeSelect seek pagination", () => {
  test("binds lastSeen_* and limit params into the SQL args", async () => {
    const store = new MockSqlStore(() => [{ id: 6 }]);
    const resolver = new MockResolver(() => store);
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: 'SELECT * FROM M WHERE "id" > ?1 ORDER BY "id" LIMIT ?2',
                  arguments: [param("lastSeen_id"), param("limit")],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    await executeSelect(plan, { lastSeen_id: 5, limit: 20 }, resolver, passthroughWrap);

    expect(store.queries).toEqual([
      {
        sql: 'SELECT * FROM M WHERE "id" > ? ORDER BY "id" LIMIT ?',
        bindings: [5, 20],
      },
    ]);
  });
});

describe("executeSelect stage/step parallelism", () => {
  /** A promise plus its resolve, so a test can control exactly when a fake call settles. */
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
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: "SELECT * FROM M",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["data"],
              query: {
                Key: {
                  database: kvDb("kv"),
                  segments: [{ Literal: "k/" }, { Value: { ParentField: "id" } }],
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    const resultPromise = executeSelect(plan, {}, resolver, passthroughWrap);

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
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: "SELECT * FROM M WHERE id = ?1",
                  arguments: [{ Param: "id" }],
                  mapping: one,
                  shard: [],
                },
              },
            },
            {
              result: ["data"],
              query: {
                Key: {
                  database: kvDb("kv"),
                  segments: [{ Literal: "k/" }, { Value: { Param: "id" } }],
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSelect(plan, { id: 7 }, resolver, passthroughWrap);
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

    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1("root"),
                  sql: "SELECT * FROM root",
                  arguments: [],
                  mapping: many,
                  shard: [],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: ["children"],
              query: {
                Sql: {
                  database: d1("children"),
                  sql: 'SELECT * FROM children WHERE "parentId" IN (?1)',
                  arguments: [spread("id")],
                  mapping: mapping("Many", [{ parent_key: "id", child_key: "parentId" }]),
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    const resultPromise = executeSelect(plan, {}, resolver, passthroughWrap);

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
  test("a missing Param throws", async () => {
    const plan: SelectPlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Sql: {
                  database: d1(),
                  sql: "SELECT ?1",
                  arguments: [param("id")],
                  mapping: one,
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    await expect(executeSelect(plan, {}, new MockResolver(), passthroughWrap)).rejects.toThrow(
      /missing parameter "id"/,
    );
  });
});

// ----------------------------------------------------------------------------
// executeSave
// ----------------------------------------------------------------------------

const write = (sql: string, args: SaveArg[] = []): SqlStatement => ({
  Write: { sql, arguments: args },
});
const hydrate = (sql: string, result: PathSegment[], args: SaveArg[] = []): SqlStatement => ({
  Hydrate: { sql, arguments: args, result },
});
const payload = (v: unknown): SaveArg => ({ Payload: v });
const resultRef = (path: PathSegment[]): SaveArg => ({ Result: path });
const field = (name: string): PathSegment => ({ Field: name });
const index = (i: number): PathSegment => ({ Index: i });

describe("executeSave SqlBatch", () => {
  test("runs Write then Hydrate and places the read-back row at the result path", async () => {
    const resolver = new MockResolver(
      () =>
        new MockSqlStore(undefined, () => [
          [], // Write returns no rows
          [{ id: 1, name: "a" }], // Hydrate read-back
        ]),
    );
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                SqlBatch: {
                  database: d1(),
                  shard: [],
                  statements: [
                    write('INSERT INTO "M" ("name") VALUES (?1)', [payload("a")]),
                    hydrate('SELECT * FROM "M" WHERE "id" = ?1', [], [payload(1)]),
                  ],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSave(plan, resolver);

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
    // the batch in order. The fake store models a store that captured last_insert_rowid()=42
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
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                SqlBatch: {
                  database: d1(),
                  shard: [],
                  statements: [
                    write('INSERT INTO "Horse" ("name") VALUES (?1)', [payload("ed")]),
                    write(
                      'INSERT OR REPLACE INTO "$cloesce_tmp" ("path", "primary_key") VALUES (\'\', json_object(\'id\', last_insert_rowid()))',
                    ),
                    hydrate(
                      'SELECT "id", "name" FROM "Horse" WHERE "id" = (SELECT json_extract("primary_key", \'$.id\') FROM "$cloesce_tmp" WHERE "path" = \'\')',
                      [],
                    ),
                  ],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSave(plan, resolver);
    expect(body).toEqual({ id: 42, name: "ed" });
  });

  test("throws when a Hydrate read-back returns no row", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(undefined, () => [[]]));
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                SqlBatch: {
                  database: d1(),
                  shard: [],
                  statements: [hydrate("SELECT * FROM M", [])],
                },
              },
            },
          ],
        },
      ],
    };
    await expect(executeSave(plan, resolver)).rejects.toThrow(/returned no row/);
  });

  test("batch failure propagates", async () => {
    const resolver = new MockResolver(
      () =>
        new MockSqlStore(undefined, () => {
          throw new Error("constraint failed");
        }),
    );
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                SqlBatch: {
                  database: d1(),
                  shard: [],
                  statements: [write("INSERT INTO M DEFAULT VALUES")],
                },
              },
            },
          ],
        },
      ],
    };
    await expect(executeSave(plan, resolver)).rejects.toThrow(/constraint failed/);
  });
});

describe("executeSave DO SqlBatch shard tagging", () => {
  test("routes to the shard stub and tags hydrated rows with route fields", async () => {
    const resolver = new MockResolver(
      () => new MockSqlStore(undefined, () => [[], [{ id: 1, name: "n" }]]),
    );
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                SqlBatch: {
                  database: doDb("agg"),
                  shard: [["tenant", payload("A")]],
                  statements: [
                    write('INSERT INTO "M" ("name") VALUES (?1)', [payload("n")]),
                    hydrate('SELECT * FROM "M" WHERE "id" = ?1', [], [payload(1)]),
                  ],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSave(plan, resolver);

    expect(body).toEqual({ id: 1, name: "n", tenant: "A" });
    expect(resolver.sqlStores.has('agg|["A"]')).toBe(true);
  });
});

describe("executeSave KeyWrite", () => {
  test("KV write records key, value, and metadata and attaches value at the result path", async () => {
    const resolver = new MockResolver();
    const key: TemplateSegment<SaveArg>[] = [
      { Literal: "profile:" },
      { Value: resultRef([field("id")]) },
    ];
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                Synthesize: {
                  fields: [["id", payload(7)]],
                  create: true,
                  cardinality: "One",
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: [field("profile")],
              query: {
                KeyWrite: {
                  database: kvDb(),
                  segments: key,
                  value: { bio: "hi" },
                  metadata: { v: 3 },
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSave(plan, resolver);

    expect(body).toEqual({ id: 7, profile: { bio: "hi" } });
    expect(resolver.keyStores.get("kv|[]")!.puts).toEqual([
      { key: "profile:7", value: { bio: "hi" }, metadata: { v: 3 } },
    ]);
  });

  test("R2-style write passes undefined metadata", async () => {
    const resolver = new MockResolver();
    const key: TemplateSegment<SaveArg>[] = [{ Literal: "blob:1" }];
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [field("blob")],
              query: {
                KeyWrite: {
                  database: r2Db(),
                  segments: key,
                  value: { bytes: [1, 2] },
                  metadata: null,
                  shard: [],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSave(plan, resolver);

    expect(body).toEqual({ blob: { bytes: [1, 2] } });
    expect(resolver.keyStores.get("r2|[]")!.puts).toEqual([
      { key: "blob:1", value: { bytes: [1, 2] }, metadata: undefined },
    ]);
  });

  test("DO-KV write routes by its shard tuple", async () => {
    const resolver = new MockResolver();
    const key: TemplateSegment<SaveArg>[] = [{ Literal: "entry:1" }];
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [field("entry")],
              query: {
                KeyWrite: {
                  database: doDb("dokv"),
                  segments: key,
                  value: { n: 1 },
                  metadata: null,
                  shard: [["tenant", payload("B")]],
                },
              },
            },
          ],
        },
      ],
    };

    await executeSave(plan, resolver);

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
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                SqlBatch: {
                  database: d1("root"),
                  shard: [],
                  statements: [
                    write("INSERT INTO root DEFAULT VALUES"),
                    hydrate("SELECT * FROM root", []),
                  ],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: [field("children"), index(0)],
              query: {
                SqlBatch: {
                  database: d1("children"),
                  shard: [],
                  statements: [
                    write("INSERT INTO children DEFAULT VALUES"),
                    hydrate("SELECT * FROM children", [field("children"), index(0)]),
                  ],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSave(plan, resolver);
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
    const plan: SavePlan = {
      stages: [
        // First stage: hydrate a "dog" body at Field("dog") holding an id.
        {
          steps: [
            {
              result: [field("dog")],
              query: {
                Synthesize: {
                  fields: [["id", payload(3)]],
                  create: true,
                  cardinality: "One",
                },
              },
            },
          ],
        },
        // Second stage: a write binding a Payload literal and a Result reference to dog.id.
        {
          steps: [
            {
              result: [],
              query: {
                SqlBatch: {
                  database: d1(),
                  shard: [],
                  statements: [
                    write('INSERT INTO "Person" ("dogId", "id") VALUES (?1, ?2)', [
                      resultRef([field("dog"), field("id")]),
                      payload(9),
                    ]),
                    hydrate('SELECT * FROM "Person" WHERE "id" = ?1', [], [payload(9)]),
                  ],
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSave(plan, resolver);

    expect(resolver.sqlStores.get("db|[]")!.batches[0].statements[0]).toEqual({
      sql: 'INSERT INTO "Person" ("dogId", "id") VALUES (?1, ?2)',
      bindings: [3, 9], // dog.id resolved from body, then the literal payload
    });
    expect(body.dog).toEqual({ id: 3 });
  });

  test("a Result reference to a missing body value throws", async () => {
    const resolver = new MockResolver();
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                SqlBatch: {
                  database: d1(),
                  shard: [],
                  statements: [
                    write("INSERT INTO M (x) VALUES (?1)", [resultRef([field("nope")])]),
                  ],
                },
              },
            },
          ],
        },
      ],
    };
    await expect(executeSave(plan, resolver)).rejects.toThrow(/missing hydrated value/);
  });
});

describe("executeSave Synthesize merge", () => {
  test("create=false merges fields into an existing body object", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(undefined, () => [[], [{ id: 1 }]]));
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                SqlBatch: {
                  database: d1(),
                  shard: [],
                  statements: [
                    write("INSERT INTO M DEFAULT VALUES"),
                    hydrate("SELECT * FROM M", []),
                  ],
                },
              },
            },
          ],
        },
        {
          steps: [
            {
              result: [],
              query: {
                Synthesize: {
                  fields: [["extra", payload("v")]],
                  create: false,
                  cardinality: "One",
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSave(plan, resolver);
    expect(body).toEqual({ id: 1, extra: "v" });
  });

  test("create=true with Many cardinality and no fields attaches an empty array", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(undefined, () => [[], [{ id: 1 }]]));
    const plan: SavePlan = {
      stages: [
        {
          steps: [
            {
              result: [],
              query: {
                SqlBatch: {
                  database: d1(),
                  shard: [],
                  statements: [
                    write("INSERT INTO M DEFAULT VALUES"),
                    hydrate("SELECT * FROM M", []),
                  ],
                },
              },
            },
            {
              result: [field("children")],
              query: {
                Synthesize: {
                  fields: [],
                  create: true,
                  cardinality: "Many",
                },
              },
            },
          ],
        },
      ],
    };

    const body = await executeSave(plan, resolver);
    expect(body).toEqual({ id: 1, children: [] });
  });
});
