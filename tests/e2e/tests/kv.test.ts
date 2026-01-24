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
    const res = await PureKVModel.SAVE({
      id,
      data: { raw: data },
      otherData: { raw: otherData },
    });
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
  });

  it("GET", async () => {
    const res = await PureKVModel.GET(id, "default");
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
    const res = await D1BackedModel.SAVE(model, "default");
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
  });

  it("GET", async () => {
    const res = await D1BackedModel.GET(1, keyParam, "default");
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(1);
    expect(res.data?.keyParam).toBe("key1");
    expect(res.data?.kvData.raw).toEqual(data);
  });

  it("LIST", async () => {
    // D1BackedModel takes a key param and thus cannot list KV components
    const res = await D1BackedModel.LIST("default");

    expect(res.ok, withRes("LIST should be OK", res)).toBe(true);
    expect(res.data.length).toBeGreaterThan(0);
    const item = res.data[0];
    expect(item.id).toBeDefined();
    expect(item.keyParam).toBeUndefined();
    expect(item.kvData).toBeUndefined();
  });
});
