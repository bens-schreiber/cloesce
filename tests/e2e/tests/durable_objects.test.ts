import { startWrangler, expectHttpResult } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { Leaderboard, Global } from "../fixtures/durable_objects/client";
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
