import { startWrangler, stopWrangler } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { Foo } from "../fixtures/middleware/client";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("./fixtures/middleware");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Global Middleware", () => {
  it("Rejects POST requests", async () => {
    const res = await Foo.SAVE({});
    expect(res.ok).toBe(false);
    expect(res.status).toBe(401);
    expect(res.message).toBe("POST methods aren't allowed.");
    expect(res.data).toBeUndefined();
  });
});

describe("Model + Method Middleware", () => {
  it("Rejects method", async () => {
    const res = await Foo.blockedMethod();

    expect(res.ok).toBe(false);
    expect(res.status).toBe(401);
    expect(res.message).toBe("Blocked method");
    expect(res.data).toBeUndefined();
  });

  it("Model middleware passes injected dep", async () => {
    const res = await Foo.getInjectedThing();

    expect(res.ok).toBe(true);
    expect(res.data).toEqual({
      value: "hello world",
    });
  });
});
