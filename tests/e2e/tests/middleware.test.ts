import { startWrangler, stopWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { House } from "../../fixtures/regression/middleware/client.js";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("../fixtures/regression/middleware");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Middleware Tests", () => {
  it("should intercept request with middleware", async () => {
    // Act - attempt to get a house, which should be intercepted by middleware
    const res = await House.get(1);

    // Assert - middleware should return 403 with custom message
    expect(res.ok, withRes("Middleware should block request", res)).toBe(false);
    expect(
      res.status,
      withRes("Middleware should return 403 status", res),
    ).toBe(403);
    expect(
      res.data,
      withRes("Middleware should return custom message", res),
    ).toBe("Should return 403 in E2E");
  });

  it("should apply middleware to all House.get calls", async () => {
    // Act - try different IDs
    const res1 = await House.get(1);
    const res2 = await House.get(999);

    // Assert - all should be blocked by middleware
    expect(res1.ok).toBe(false);
    expect(res1.status).toBe(403);
    expect(res1.data).toBe("Should return 403 in E2E");

    expect(res2.ok).toBe(false);
    expect(res2.status).toBe(403);
    expect(res2.data).toBe("Should return 403 in E2E");
  });
});
