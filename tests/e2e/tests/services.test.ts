import { startWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { FooService } from "../fixtures/services/client";
import config from "../fixtures/services/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/services", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("FooService", () => {
  it("GET Request", async () => {
    const res = await FooService.method();
    expect(res.ok, withRes("Expected GET to work", res)).toBe(true);
    expect(res.data).toEqual("foo's invocation; injected: injected value");
  });
});
