import { startWrangler, expectHttpResult } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { PooAcceptYield } from "../fixtures/poos/client";
import config from "../fixtures/poos/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/poos", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Upload", () => {
  it("accepts Poos", async () => {
    const res = await PooAcceptYield.acceptPoos(
      {
        name: "test-name",
        major: "test-major",
      },
      {
        color: "test-color",
      },
      {
        a: {
          name: "test-name",
          major: "test-major",
        },
        b: [
          {
            color: "test-color",
          },
        ],
      },
    );
    expectHttpResult(res, "POST should be OK");
  });
});

describe("Receive", () => {
  it("yields Poo", async () => {
    const res = await PooAcceptYield.yieldPoo();

    expectHttpResult(res, "POST should be OK");
    expect(res.data).toEqual({
      a: {
        name: "name",
        major: "major",
      },
      b: [
        {
          color: "color",
        },
      ],
    });
  });
});
