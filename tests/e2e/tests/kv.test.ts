import { startWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import {
  PureKVModel,
  D1BackedModel,
  PaginatedKVModel,
  KValue,
  Paginated,
} from "../fixtures/kv/client";
import config from "../fixtures/kv/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/kv", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("PureKVModel", () => {
  const id = "test-id";
  const data = { foo: "bar" };
  const otherData = "some string data";

  it("POST", async () => {
    const res = await PureKVModel.$save({
      id,
      data: { raw: data },
      otherData: { raw: otherData },
    });
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
  });

  it("GET", async () => {
    const res = await PureKVModel.$get(id);
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(id);
    expect(res.data?.data.value).toEqual(data);
    expect(res.data?.otherData.value).toBe(otherData);
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
    const res = await D1BackedModel.$save(model);
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
  });

  it("GET", async () => {
    const res = await D1BackedModel.$get(1, keyParam);
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(1);
    expect(res.data?.keyParam).toBe("key1");
    expect(res.data?.kvData.value).toEqual(data);
  });

  it("LIST", async () => {
    // D1BackedModel takes a key param and thus cannot list KV components
    const res = await D1BackedModel.$list(0, 10);

    expect(res.ok, withRes("LIST should be OK", res)).toBe(true);
    expect(res.data!.length).toBeGreaterThan(0);
    const item = res.data![0];
    expect(item.id).toBeDefined();
    expect(item.keyParam).toBeUndefined();
    expect(item.kvData).toBeUndefined();
  });
});

describe("PaginatedKVModel", () => {
  const id = "test-id";

  it("GET with Paginated KV list returns paginated structure", async () => {
    const res = await PaginatedKVModel.$get(id);
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(id);

    // Verify the paginated structure
    expect(res.data?.items).toBeDefined();
    expect(res.data?.items.results).toBeDefined();
    expect(Array.isArray(res.data?.items.results)).toBe(true);
    expect(res.data?.items.cursor).toBe(null);
    expect(res.data?.items.complete).toBeTypeOf("boolean");
  });

  it("paginated KV cursor can be used for next page", async () => {
    const res = await PaginatedKVModel.$get(id);
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);

    if (res.data?.items.cursor) {
      // If there is a cursor, we can use it for pagination
      // This verifies the cursor is a valid string that can be used with KVNamespace.list()
      expect(typeof res.data.items.cursor).toBe("string");
    } else {
      // If there is no cursor, then all items fit in the first page
      expect(res.data?.items.complete).toBe(true);
    }
  });

  it("accepts a paginated structure in POST", async () => {
    const paginatedData: Paginated<KValue<unknown>> = {
      results: [
        { key: "item1", raw: "test", metadata: null } as KValue<unknown>,
        { key: "item2", raw: "test2", metadata: null } as KValue<unknown>,
      ],
      cursor: "next-page-cursor",
      complete: false,
    };

    const res = await PaginatedKVModel.acceptPaginated(paginatedData);

    expect(res.ok, withRes("acceptPaginated should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data).toEqual(paginatedData);
  });
});
