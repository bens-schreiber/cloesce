import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, stopWrangler, withRes } from "../src/setup";
import { CrudHaver } from "../../fixtures/regression/crud/client";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("../fixtures/regression/crud");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Upserts", () => {
  let model: CrudHaver;
  it("POST", async () => {
    const res = await CrudHaver.post({
      name: "tim",
    });

    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data).toEqual({
      id: 1,
      name: "tim",
    });
    model = res.data;
  });

  it("PATCH", async () => {
    model.name = "julio";
    const res = await model.patch();

    expect(res.ok, withRes("PATCH should be OK", res)).toBe(true);
    expect(model).toEqual({
      id: 1,
      name: "julio",
    });
  });
});
