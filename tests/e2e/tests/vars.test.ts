import { startWrangler, expectHttpResult } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { Session } from "../fixtures/vars/client";
import config from "../fixtures/vars/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/vars", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Session", () => {
  it("injects a var into the route env", async () => {
    const res = await Session.login("me@example.com");
    expectHttpResult(res, "Expected login to work");
    expect(res.data!.value).toEqual("me@example.com:default_string");
  });
});
