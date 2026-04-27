import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, withRes } from "../src/setup";
import { Weather } from "../fixtures/bools_dates_ints/client";
import config from "../fixtures/bools_dates_ints/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/bools_dates_ints", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Bools, Dates, Ints", () => {
  it("POST", async () => {
    const date = new Date("2020-01-01").toISOString();
    const res = await Weather.$save({
      Default: {
        isRaining: true,
        date: date,
      },
    });
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data).toEqual({
      id: 1,
      isRaining: true,
      date,
    });
  });
});
