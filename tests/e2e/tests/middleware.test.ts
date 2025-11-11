import { startWrangler, stopWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { Model } from "../../fixtures/regression/middleware/client.js";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("../fixtures/regression/middleware");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

function makeFetchImpl() {
  const fetchImpl: typeof fetch & { lastResponse?: Response } = async (
    url,
    options,
  ) => {
    const response = await fetch(url, options);
    const clone = response.clone();

    fetchImpl.lastResponse = response;
    return response;
  };

  return fetchImpl;
}

describe("Global Middleware", () => {
  it("Rejects POST requests", async () => {
    const fetchImpl = makeFetchImpl();

    const res = await Model.save({}, "none", fetchImpl);
    expect(res.ok).toBe(false);
    expect(res.status).toBe(401);
    expect(res.message).toBe("POST methods aren't allowed.");
    expect(res.data).toBeUndefined();
    expect(fetchImpl.lastResponse?.headers.get("X-Cloesce-Test")).toBe("true");
  });
});

describe("Model + Method Middleware", () => {
  it("Rejects method", async () => {
    const fetchImpl = makeFetchImpl();
    const res = await Model.blockedMethod(fetchImpl);

    expect(res.ok).toBe(false);
    expect(res.status).toBe(401);
    expect(res.message).toBe("Blocked method");
    expect(res.data).toBeUndefined();
    expect(fetchImpl.lastResponse?.headers.get("X-Cloesce-Test")).toBe("true");
  });

  it("Model middleware passes injected dep", async () => {
    const fetchImpl = makeFetchImpl();
    const res = await Model.getInjectedThing(fetchImpl);

    expect(res.ok).toBe(true);
    expect(res.data).toEqual({
      value: "hello world",
    });
    expect(fetchImpl.lastResponse?.headers.get("X-Cloesce-Test")).toBe("true");
  });
});
