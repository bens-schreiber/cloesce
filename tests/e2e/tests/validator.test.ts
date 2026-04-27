import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, withRes } from "../src/setup";
import { Validator } from "../fixtures/validators/client";
import config from "../fixtures/validators/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/validators", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Validator Tests", () => {
  it("save fails when id is too high", async () => {
    const res = await Validator.$save({
      Default: {
        id: 150,
        email: "test@example.com",
      },
    });
    expect(res.ok, withRes("Expected validation to fail", res)).toBe(false);
    expect(res.status).toBe(400);
  });

  it("save fails when email is too small", async () => {
    const res = await Validator.$save({
      Default: {
        id: 50,
        email: "a@b.c",
      },
    });
    expect(res.ok, withRes("Expected validation to fail", res)).toBe(false);
    expect(res.status).toBe(400);
  });

  it("save fails when email is too large", async () => {
    const longEmail = "a".repeat(300) + "@example.com";
    const res = await Validator.$save({
      Default: {
        id: 50,
        email: longEmail,
      },
    });
    expect(res.ok, withRes("Expected validation to fail", res)).toBe(false);
    expect(res.status).toBe(400);
  });

  it("save fails when email is not an email", async () => {
    const res = await Validator.$save({
      Default: {
        id: 50,
        email: "not-an-email",
      },
    });
    expect(res.ok, withRes("Expected validation to fail", res)).toBe(false);
    expect(res.status).toBe(400);
  });

  it("save fails when name is not length 10", async () => {
    const res = await Validator.$save({
      Default: {
        id: 50,
        email: "test@example.com",
        name: "short",
      },
    });
    expect(res.ok, withRes("Expected validation to fail", res)).toBe(false);
    expect(res.status).toBe(400);
  });

  it("save fails when KV value is over length 500", async () => {
    const longValue = "a".repeat(501);
    const res = await Validator.$save({
      Default: {
        id: 50,
        email: "test@example.com",
        name: "testuser",
        data: {
          raw: longValue,
        },
      },
    });
    expect(res.ok, withRes("Expected validation to fail", res)).toBe(false);
    expect(res.status).toBe(400);
  });

  let validator: Validator;
  it("save succeeds when all fields are valid", async () => {
    const res = await Validator.$save({
      Default: {
        id: 50,
        email: "test@example.com",
        name: "testuser12",
        data: {
          raw: "valid data",
        },
      },
    });
    expect(res.ok, withRes("Expected validation to succeed", res)).toBe(true);
    expect(res.status).toBe(200);

    validator = res.data!;
  });

  it("api method fails when id is too high", async () => {
    const res = await validator.someMethod(150, "testuser");
    expect(res.ok, withRes("Expected API call to fail", res)).toBe(false);
    expect(res.status).toBe(400);
  });

  it("api method fails when name is not length 10", async () => {
    const res = await validator.someMethod(50, "short");
    expect(res.ok, withRes("Expected API call to fail", res)).toBe(false);
    expect(res.status).toBe(400);
  });

  it("api method succeeds when all fields are valid", async () => {
    const res = await validator.someMethod(50, "testuser12");
    expect(res.ok, withRes("Expected API call to succeed", res)).toBe(true);
    expect(res.status).toBe(200);
  });
});
