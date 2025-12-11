import { startWrangler, stopWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { Dog } from "../../fixtures/regression/partials/client.js";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("../fixtures/regression/partials");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Dog", () => {
  let dog: Dog | undefined;
  it("Partial", async () => {
    const res = await Dog.post({
      name: "fido",
      age: 100,
    });

    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data).toEqual({
      id: 1,
      name: "fido",
      age: 100,
    });

    dog = res.data;
  });

  it("Full", async () => {
    const res = await Dog.post({
      id: 2,
      name: "fido",
      age: 100,
    });

    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data).toEqual({
      id: 2,
      name: "fido",
      age: 100,
    });
  });

  it("Get Partial Self", async () => {
    const res = await dog!.getPartialSelf();
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toEqual({
      name: "fido",
    });
  });
});
