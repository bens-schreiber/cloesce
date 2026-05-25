import { startWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { ModelWithKv, KValue, Paginated } from "../fixtures/kv/client";
import config from "../fixtures/kv/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/kv", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("ModelWithKv", () => {
  const id = 1;
  const someData = { foo: "bar" };
  const someOtherData = "some string data";

  it("POST", async () => {
    const model = {
      id,
      someColumn: 42,
      someOtherColumn: "hello",
      someData: { raw: someData },
      someOtherData: { raw: someOtherData },
    };
    const res = await ModelWithKv.$save(model);
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
  });

  it("GET", async () => {
    const res = await ModelWithKv.$get(id);
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(id);
    expect(res.data?.someData.value).toEqual(someData);
    expect(res.data?.someOtherData.value).toBe(someOtherData);
  });

  it("LIST", async () => {
    const res = await ModelWithKv.$list(0, 10);
    expect(res.ok, withRes("LIST should be OK", res)).toBe(true);
    expect(res.data!.length).toBeGreaterThan(0);
    const item = res.data![0];
    expect(item.id).toBeDefined();
  });

  it("GET with paginated KV list returns paginated structure", async () => {
    const res = await ModelWithKv.$get(id);
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();

    expect(res.data?.paginatedItems).toBeDefined();
    expect(res.data?.paginatedItems.results).toBeDefined();
    expect(Array.isArray(res.data?.paginatedItems.results)).toBe(true);
    expect(res.data?.paginatedItems.complete).toBeTypeOf("boolean");
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

    const res = await ModelWithKv.acceptPaginated(paginatedData);

    expect(res.ok, withRes("acceptPaginated should be OK", res)).toBe(true);
    expect(res.data).toBeDefined();
    expect(res.data).toEqual(paginatedData);
  });
});
