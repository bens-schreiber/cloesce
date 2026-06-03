import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, withRes } from "../src/setup";
import { Weather } from "../fixtures/bools_dates/client";
import config from "../fixtures/bools_dates/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/bools_dates", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Bools, Dates", () => {
  it("POST", async () => {
    const date = new Date("2020-01-01");
    const res = await Weather.$save({
      isRaining: true,
      date,
    });
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data).toEqual({
      id: 1,
      isRaining: true,
      date,
    });
  });

  it("Returns 2026 date", async () => {
    const res = await Weather.getCurrentDate();
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toEqual(new Date("2026-01-01T00:00:00.000Z"));
  });

  it("Returns true for isItRainingSomewhere", async () => {
    const res = await Weather.isItRainingSomewhere();
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBe(true);
  });

  it("Echoes bools and dates properly", async () => {
    const date = new Date("2020-01-01");
    const res = await Weather.echo(date, true);
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toEqual({
      id: 1,
      isRaining: true,
      date,
    });
  });
});
