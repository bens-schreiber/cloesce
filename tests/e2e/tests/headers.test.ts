import { startWrangler, expectHttpResult } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { HeaderService } from "../fixtures/headers/client";
import config from "../fixtures/headers/cloesce.jsonc" with { type: "jsonc" };

const workersUrl = config.workers_url!;

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/headers", workersUrl);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("HeaderService", () => {
  it("round-trips a header param through the generated client", async () => {
    const res = await HeaderService.echo("vox", "machina");
    expectHttpResult(res, "Expected header param to round-trip");
    expect(res.data).toEqual("vox:machina");
  });

  it("coerces a typed (int) header on a GET", async () => {
    const res = await HeaderService.count(41);
    expectHttpResult(res, "Expected int header to coerce");
    expect(res.data).toEqual(42);
  });

  it("400s when a required header is missing", async () => {
    const res = await fetch(`${workersUrl}/HeaderService/echo`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ payload: "hello" }),
    });
    expect(res.status).toBe(400);
  });

  it("reads the value from the header, not the body", async () => {
    const res = await fetch(`${workersUrl}/HeaderService/echo`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "X-Tenant": "vox" },
      body: JSON.stringify({ payload: "machina", X_Tenant: "mighty" }),
    });
    expect(res.status).toBe(200);

    const body = await res.json();
    expect(body).toEqual("vox:machina");
  });
});
