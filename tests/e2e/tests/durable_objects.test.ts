import { startWrangler, expectHttpResult } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import {
  Leaderboard,
  Global,
  LeaderboardEntry,
  PlayerScore,
} from "../fixtures/durable_objects/client";
import config from "../fixtures/durable_objects/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/durable_objects", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Sharded Durable Object", () => {
  it("setScore executes in the DO and persists to its storage", async () => {
    const res = await Leaderboard.setScore(1, 100);
    expectHttpResult(res, "setScore should be OK");
  });

  it("getScore reads back the value written in the same shard", async () => {
    const res = await Leaderboard.getScore(1);
    expectHttpResult(res, "getScore should be OK");
    expect(res.data).toBe(100);
  });

  it("different tenantIds resolve to isolated DO instances", async () => {
    // tenant 2 has never been written to.
    const before = await Leaderboard.getScore(2);
    expectHttpResult(before, "getScore(2) should be OK");
    expect(before.data).toBe(0);

    // Writing tenant 2 must not affect tenant 1's shard.
    await Leaderboard.setScore(2, 55);

    const stillTenant1 = await Leaderboard.getScore(1);
    expect(stillTenant1.data).toBe(100);

    const tenant2 = await Leaderboard.getScore(2);
    expect(tenant2.data).toBe(55);
  });

  it("rejects a tenantId that violates the inherited shard validator", async () => {
    // tenantId inherits `[gt 0]` from the shard field.
    const res = await Leaderboard.getScore(0);
    expect(res.ok, `getScore(0) should fail validation\n\n${JSON.stringify(res)}`).toBe(false);
    expect(res.status).toBe(400);
  });
});

describe("Global Durable Object", () => {
  it("setConfig / getConfig execute in the single global DO", async () => {
    const set = await Global.setConfig("hello");
    expectHttpResult(set, "setConfig should be OK");

    const get = await Global.getConfig();
    expectHttpResult(get, "getConfig should be OK");
  });
});

describe("Durable Object-backed model", () => {
  it("a single $save fans out KV writes to the DO storage and a Worker KV namespace", async () => {
    const saved = await LeaderboardEntry.$save(1, {
      tenantId: 1,
      rank: 5,
      score: { raw: 250 },
      meta: { raw: "gold" },
    });
    expectHttpResult(saved, "$save should be OK");

    // $get forwards into the same DO and hydrates `score` from DO storage and `meta`
    // from the Worker KV namespace.
    const got = await LeaderboardEntry.$get(1, 5);
    expectHttpResult(got, "$get should be OK");
    expect(got.data!.tenantId).toBe(1);
    expect(got.data!.rank).toBe(5);
    expect(got.data!.score.value).toBe(250);
    expect(got.data!.meta.value).toBe("gold");
  });

  it("different tenants resolve to isolated DO instances", async () => {
    await LeaderboardEntry.$save(3, {
      tenantId: 3,
      rank: 1,
      score: { raw: 70 },
      meta: { raw: "bronze" },
    });

    const tenant3 = await LeaderboardEntry.$get(3, 1);
    expect(tenant3.data!.score.value).toBe(70);

    // tenant 1 is unaffected by tenant 3's write.
    const tenant1 = await LeaderboardEntry.$get(1, 5);
    expect(tenant1.data!.score.value).toBe(250);
  });
});

describe("SQL-backed Durable Object model", () => {
  it("$save inserts a row into the DO's SQLite database (migration applied on construction)", async () => {
    const saved = await PlayerScore.$save(1, { playerName: "alice", points: 100 });
    expectHttpResult(saved, "$save should be OK");
    expect(saved.data!.id).toBeTypeOf("number");
    expect(saved.data!.playerName).toBe("alice");
    expect(saved.data!.points).toBe(100);
    expect(saved.data!.tenantId).toBe(1);
  });

  it("$get fetches a row by primary key inside the DO", async () => {
    const saved = await PlayerScore.$save(1, { playerName: "bob", points: 55 });
    expectHttpResult(saved, "$save should be OK");

    const got = await PlayerScore.$get(1, saved.data!.id);
    expectHttpResult(got, "$get should be OK");
    expect(got.data!.id).toBe(saved.data!.id);
    expect(got.data!.playerName).toBe("bob");
    expect(got.data!.points).toBe(55);
    expect(got.data!.tenantId).toBe(1);
  });

  it("$save with a primary key updates the existing row", async () => {
    const saved = await PlayerScore.$save(1, { playerName: "carol", points: 10 });
    expectHttpResult(saved, "$save should be OK");

    const updated = await PlayerScore.$save(1, {
      id: saved.data!.id,
      playerName: "carol",
      points: 999,
    });
    expectHttpResult(updated, "$save update should be OK");
    expect(updated.data!.id).toBe(saved.data!.id);
    expect(updated.data!.points).toBe(999);
  });

  it("$list seek-paginates rows within the DO", async () => {
    const all = await PlayerScore.$list(1, 0, 100);
    expectHttpResult(all, "$list should be OK");
    expect(all.data!.length).toBeGreaterThanOrEqual(3);

    // Seek past the first row.
    const firstId = all.data![0].id;
    const rest = await PlayerScore.$list(1, firstId, 100);
    expectHttpResult(rest, "$list after firstId should be OK");
    expect(rest.data!.length).toBe(all.data!.length - 1);
    expect(rest.data!.every((row) => row.id > firstId)).toBe(true);
  });

  it("rows are isolated per tenant shard", async () => {
    await PlayerScore.$save(9, { playerName: "tenant9", points: 1 });

    const tenant9 = await PlayerScore.$list(9, 0, 100);
    expectHttpResult(tenant9, "$list(9) should be OK");
    expect(tenant9.data!.length).toBe(1);
    expect(tenant9.data![0].playerName).toBe("tenant9");

    // tenant 9's database does not contain tenant 1's rows.
    expect(tenant9.data!.some((row) => row.playerName === "alice")).toBe(false);
  });

  it("$get for a missing row returns 404", async () => {
    const res = await PlayerScore.$get(1, 999999);
    expect(res.ok, `expected 404\n\n${JSON.stringify(res)}`).toBe(false);
    expect(res.status).toBe(404);
  });
});
