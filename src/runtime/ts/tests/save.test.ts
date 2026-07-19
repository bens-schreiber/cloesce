import { describe, test, expect } from "vitest";
import { executeSave } from "../src/router/executor/index.js";
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
  batchStep,
  executeSaveOk,
  field,
  hydrate,
  index,
  keyWriteStep,
  payload,
  resultRef,
  savePlan,
  saveSynthStep,
  write,
} from "./common/save.js";

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

  test("a Hydrate read-back with no row attaches nothing, silently", async () => {
    const resolver = new MockResolver(() => new MockSqlStore(undefined, () => [[]]));
    const plan = savePlan([batchStep([], [hydrate("SELECT * FROM M", [])])]);

    const res = await executeSave(plan, resolver);

    expect(res.value).toBeNull();
    expect(res.errors).toEqual([]);
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
      [
        keyWriteStep([field("blob")], kvDb(), [{ Literal: "k/1" }], {
          ok: true,
        }),
      ],
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
      keyWriteStep([field("blob")], r2Db(), [{ Literal: "blob:1" }], {
        bytes: [1, 2],
      }),
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

  test("a Result reference to a missing body value skips the batch, silently", async () => {
    const resolver = new MockResolver();
    const plan = savePlan([
      batchStep([], [write("INSERT INTO M (x) VALUES (?1)", [resultRef([field("nope")])])]),
    ]);

    const res = await executeSave(plan, resolver);

    expect(res.value).toBeNull();
    expect(res.errors).toEqual([]);
    expect(resolver.sqlStores.size).toBe(0);
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
