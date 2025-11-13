import { startWrangler, stopWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { PooAcceptYield } from "../../fixtures/regression/poos/client.js";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("../fixtures/regression/poos");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("POST PooAcceptYield", () => {
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

    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
  });
});

describe("yieldPoo", () => {
  it("yields Poo", async () => {
    const res = await PooAcceptYield.yieldPoo();

    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
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
