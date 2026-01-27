import { startWrangler, stopWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { PureR2Model, D1BackedModel } from "../fixtures/r2/client";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("./fixtures/r2");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Pure R2 Model", () => {
  it("uploads data", async () => {
    const model = Object.assign(new PureR2Model(), {
      id: "test-id-1",
    });

    const res = await model.uploadData(new TextEncoder().encode("Hello, R2!"));

    expect(res.ok, withRes("PUT should be OK", res)).toBe(true);
  });

  it("uploads other data", async () => {
    const model = Object.assign(new PureR2Model(), {
      id: "test-id-1",
    });

    const res = await model.uploadOtherData(
      new TextEncoder().encode("Hello, R2!"),
    );

    expect(res.ok, withRes("PUT should be OK", res)).toBe(true);
  });

  it("retrieves head", async () => {
    const res = await PureR2Model.GET("test-id-1", "default");
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe("test-id-1");

    expect(res.data?.data.key).toBe("path/to/data/test-id-1");
    expect(res.data?.otherData.key).toBe("path/to/other/test-id-1");
    expect(res.data?.allData.length).toBe(2);

    expect(res.data?.allData.map((obj) => obj.key).sort()).toEqual([
      "path/to/data/test-id-1",
      "path/to/other/test-id-1",
    ]);
  });
});

describe("D1 Backed Model", () => {
  let model: D1BackedModel;
  it("uploads d1", async () => {
    const res = await D1BackedModel.SAVE({
      keyParam: "key-param-1",
      someColumn: 42,
      someOtherColumn: "foo",
    });

    expect(res.ok, withRes("SAVE should be OK", res)).toBe(true);
    model = res.data!;
  });

  it("uploads r2 data", async () => {
    const res = await model.uploadData(
      new TextEncoder().encode("D1 Backed R2 Data"),
    );
    expect(res.ok, withRes("PUT should be OK", res)).toBe(true);
  });

  it("retrieves full model", async () => {
    const res = await D1BackedModel.GET(model.id, model.keyParam, "default");
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(model.id);
    expect(res.data?.keyParam).toBe(model.keyParam);
    expect(res.data?.r2Data.key).toBe("d1Backed/1/key-param-1/42/foo");
  });

  it("lists models", async () => {
    const res = await D1BackedModel.LIST("default");
    expect(res.ok, withRes("LIST should be OK", res)).toBe(true);
    expect(res.data!.length).toBeGreaterThan(0);

    const found = res.data!.find((m) => m.id === model.id)!;
    expect(found).toBeDefined();
    expect(found.r2Data).toBeUndefined(); // model takes a keyparam and thus cannot list R2 components
  });
});
