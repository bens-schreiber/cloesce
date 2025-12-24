import { startWrangler, stopWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { TextKV } from "../../fixtures/regression/kv/client.js";

beforeAll(async () => {
    // NOTE: e2e is called from proj root
    await startWrangler("../fixtures/regression/kv", false);
}, 30_000);

afterAll(async () => {
    await stopWrangler();
});

describe("TextKV", () => {
    const key = "test-key";
    const value = "test-value";

    it("Performs PUT method", async () => {
        const res = await TextKV.put(key, value);
        expect(res.ok, withRes("PUT failed", res)).toBe(true);
    });

    let kv: TextKV;
    it("Retrieves TextKV via GET method", async () => {
        const res = await TextKV.get(key);
        expect(res.ok, withRes("GET failed", res)).toBe(true);

        kv = res.data;

        expect(kv instanceof TextKV).toBe(true);
        expect(kv.value).toBe(value);
        expect(kv.key).toBe(key);
        expect(kv.metadata).toBeDefined();
    });

    it("Deletes TextKV via DELETE method", async () => {
        const res = await kv.delete();
        expect(res.ok, withRes("DELETE failed", res)).toBe(true);
    });

    it("Verifies deletion", async () => {
        const res = await kv.delete(); // call another instance method to verify 404
        expect(res.status, withRes("Expected 404", res)).toBe(404);
    });
});