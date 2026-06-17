import { startWrangler, expectHttpResult } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { ModelWithKv, KVOnly, KValue, KVOnlyWithSingleton } from "../fixtures/kv/client";
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
    expectHttpResult(res, "POST should be OK");
  });

  it("GET", async () => {
    const res = await ModelWithKv.$get(id);
    expectHttpResult(res, "GET should be OK");
    expect(res.data).toBeDefined();
    expect(res.data?.id).toBe(id);
    expect(res.data?.someData.value).toEqual(someData);
    expect(res.data?.someOtherData.value).toBe(someOtherData);
  });

  it("LIST", async () => {
    const res = await ModelWithKv.$list(0, 10);
    expectHttpResult(res, "LIST should be OK");
    expect(res.data!.length).toBeGreaterThan(0);
    const item = res.data![0];
    expect(item.id).toBeDefined();
  });

  it("accepts a KValue in POST", async () => {
    const item = { raw: "test", metadata: null } as KValue<unknown>;

    const res = await ModelWithKv.acceptKvObject(item);

    expectHttpResult(res, "acceptKvObject should be OK");
    expect(res.data).toBeDefined();
    expect(res.data).toEqual(item);
  });
});

describe("KVOnly (route model)", () => {
  const id = 7;
  const someData = { hello: "world" };
  const siblingData = "sibling string data";

  it("POST persists the route model's and its nav target's KV fields", async () => {
    const model = {
      id,
      someData: { raw: someData },
      sibling: {
        siblingId: id,
        siblingData: { raw: siblingData },
      },
    };
    const res = await KVOnly.$save(model);
    expectHttpResult(res, "POST should be OK");
  });

  it("GET hydrates KV and the assembled route nav with its KV", async () => {
    const res = await KVOnly.$get(id);
    expectHttpResult(res, "GET should be OK");
    expect(res.data?.id).toBe(id);
    expect(res.data?.someData.value).toEqual(someData);

    // The sibling is assembled from this model's route fields, then its KV hydrated.
    expect(res.data?.sibling).toBeDefined();
    expect(res.data?.sibling?.siblingId).toBe(id);
    expect(res.data?.sibling?.siblingData.value).toBe(siblingData);
  });
});

describe("KVOnlyWithSingleton (keyless singleton nav)", () => {
  const id = 3;
  const someData = { hello: "singleton" };
  const configData = { appName: "cloesce" };

  it("POST persists the model's KV and the singleton nav's KV", async () => {
    const model = {
      id,
      someData: { raw: someData },
      appConfig: {
        config: { raw: configData },
      },
    };
    const res = await KVOnlyWithSingleton.$save(model);
    expectHttpResult(res, "POST should be OK");
  });

  it("GET hydrates the singleton nav alongside the model's own KV", async () => {
    const res = await KVOnlyWithSingleton.$get(id);
    expectHttpResult(res, "GET should be OK");
    expect(res.data?.id).toBe(id);
    expect(res.data?.someData.value).toEqual(someData);
    expect(res.data?.appConfig).toBeDefined();
    expect(res.data?.appConfig?.config.value).toEqual(configData);
  });
});
