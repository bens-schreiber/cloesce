import { startWrangler, expectHttpResult } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { D1BackedModel, R2Only, R2Sibling } from "../fixtures/r2/client";
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

    expectHttpResult(res, "SAVE should be OK");
    model = res.data!;
  });

  it("uploads r2 data", async () => {
    const res = await model.uploadData(new TextEncoder().encode("D1 Backed R2 Data"));
    expectHttpResult(res, "PUT should be OK");
  });

  it("uploads r2 other data", async () => {
    const res = await model.uploadOtherData(new TextEncoder().encode("Other R2 Data"));
    expectHttpResult(res, "PUT should be OK");
  });

  it("retrieves full model", async () => {
    const res = await D1BackedModel.$get(model.id);
    expectHttpResult(res, "GET should be OK");
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(model.id);
    expect(res.data?.someData?.key).toBe(`path/to/data/${model.id}`);
    expect(res.data?.someOtherData?.key).toBe(`path/to/other/${model.id}`);
  });

  it("lists models", async () => {
    const res = await D1BackedModel.$list(0, 10);
    expectHttpResult(res, "LIST should be OK");
    expect(res.data!.length).toBeGreaterThan(0);

    const found = res.data!.find((m) => m.id === model.id)!;
    expect(found).toBeDefined();
  });
});

describe("R2Only (route model)", () => {
  const id = 9;

  it("uploads its own and its nav target's r2 data", async () => {
    const only = Object.assign(new R2Only(), { id });
    const res = await only.uploadData(new TextEncoder().encode("R2Only Data"));
    expectHttpResult(res, "PUT should be OK");

    const sibling = Object.assign(new R2Sibling(), { siblingId: id });
    const sibRes = await sibling.uploadData(new TextEncoder().encode("R2Sibling Data"));
    expectHttpResult(sibRes, "sibling PUT should be OK");
  });

  it("GET hydrates r2 and the assembled route nav with its r2", async () => {
    const res = await R2Only.$get(id);
    expectHttpResult(res, "GET should be OK");
    expect(res.data?.id).toBe(id);
    expect(res.data?.someData?.key).toBe(`path/to/data/${id}`);

    // The sibling is assembled from this model's route fields, then its r2 hydrated.
    expect(res.data?.sibling).toBeDefined();
    expect(res.data?.sibling?.siblingId).toBe(id);
    expect(res.data?.sibling?.siblingData?.key).toBe(`path/to/other/${id}`);
  });
});
