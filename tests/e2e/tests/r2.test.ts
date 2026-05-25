import { startWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { D1BackedModel } from "../fixtures/r2/client";
import config from "../fixtures/r2/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/r2", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("D1 Backed Model", () => {
  let model: D1BackedModel;
  it("saves model", async () => {
    const res = await D1BackedModel.$save({
      id: 1,
      someColumn: 42,
      someOtherColumn: "foo",
    });

    expect(res.ok, withRes("SAVE should be OK", res)).toBe(true);
    model = res.data!;
  });

  it("uploads r2 data", async () => {
    const res = await model.uploadData(new TextEncoder().encode("D1 Backed R2 Data"));
    expect(res.ok, withRes("PUT should be OK", res)).toBe(true);
  });

  it("uploads r2 other data", async () => {
    const res = await model.uploadOtherData(new TextEncoder().encode("Other R2 Data"));
    expect(res.ok, withRes("PUT should be OK", res)).toBe(true);
  });

  it("retrieves full model", async () => {
    const res = await D1BackedModel.$get(model.id);
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(model.id);
    expect(res.data?.someData?.key).toBe(`path/to/data/${model.id}`);
    expect(res.data?.someOtherData?.key).toBe(`path/to/other/${model.id}`);
  });

  it("lists models", async () => {
    const res = await D1BackedModel.$list(0, 10);
    expect(res.ok, withRes("LIST should be OK", res)).toBe(true);
    expect(res.data!.length).toBeGreaterThan(0);

    const found = res.data!.find((m) => m.id === model.id)!;
    expect(found).toBeDefined();
  });
});
