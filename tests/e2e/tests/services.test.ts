import { startWrangler, stopWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import {
  FooService,
  BarService,
} from "../../fixtures/regression/services/client.js";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("../fixtures/regression/services", false);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Static, Instantiated Methods: FooService", () => {
  it("Static GET Request", async () => {
    const res = await FooService.staticMethod();
    expect(res.ok, withRes("Expected GET to work", res)).toBe(true);
    expect(res.data).toEqual("foo's static invocation");
  });

  it("Instantiated GET Request", async () => {
    const res = await FooService.instantiatedMethod();
    expect(res.ok, withRes("Expected GET to work", res)).toBe(true);
    expect(res.data).toEqual("foo's instantiated invocation");
  });
});

describe("Use Injected Dependency: BarService", () => {
  it("Returns Foo's instantiated method", async () => {
    const res = await BarService.useFoo();
    expect(res.ok, withRes("Expected GET to work", res)).toBe(true);
    expect(res.data).toEqual("foo's instantiated invocation from BarService");
  });
});
