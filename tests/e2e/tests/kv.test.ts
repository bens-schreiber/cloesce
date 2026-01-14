import { startWrangler, stopWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import {
  PureKVModel,
  D1BackedModel,
} from "../../fixtures/regression/kv/client.js";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("../fixtures/regression/kv");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("PureKVModel", () => {
  const id = "test-id";
  const data = { foo: "bar" };
  const otherData = "some string data";

  it("POST", async () => {
    const res = await PureKVModel.save({
      id,
      data: { raw: data },
      otherData: { raw: otherData },
    });
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
  });

  it("GET", async () => {
    const res = await PureKVModel.get(id, "default");
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(id);
    expect(res.data?.data.raw).toEqual(data);
    expect(res.data?.otherData.raw).toBe(otherData);
  });
});

describe("D1BackedModel", () => {
  const data = { nested: "data" };
  const keyParam = "key1";

  it("POST", async () => {
    const model = {
      keyParam,
      someColumn: 42,
      someOtherColumn: "hello",
      kvData: {
        raw: data,
      },
    };
    const res = await D1BackedModel.save(model, "default");
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
  });

  it("GET", async () => {
    const res = await D1BackedModel.get(1, keyParam, "default");
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(1);
    expect(res.data?.keyParam).toBe("key1");
    expect(res.data?.kvData.raw).toEqual(data);
  });
});
