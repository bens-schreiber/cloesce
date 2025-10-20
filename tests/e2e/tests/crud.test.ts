import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, stopWrangler, withRes } from "../src/setup";
import { CrudHaver } from "../../fixtures/regression/crud/client";
import { T } from "vitest/dist/chunks/reporters.d.BFLkQcL6";

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

describe("GET", () => {
  it("GET a model", async () => {
    const res = await CrudHaver.get(1);
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toEqual({
      id: 1,
      name: "julio",
    });
  });
});

describe("List", () => {
  const models = ["a", "b", "c"];

  it("POST 3 Models", async () => {
    await Promise.all(
      models.map(async (m) => {
        const res = await CrudHaver.post({ name: m });
        expect(res.ok, withRes("POST should be OK", res)).toBe(true);
        expect(res.data.name).toEqual(m);
      })
    );
  });

  it("List 3 models", async () => {
    const res = await CrudHaver.list();
    expect(res.ok, withRes("LIST should be OK", res)).toBe(true);
    expect(res.data.length, withRes("Should be 4 ites", res)).toBe(4); // including the one from the prev test
    models.forEach((m) =>
      expect(res.data.map((d: CrudHaver) => d.name)).toContain(m)
    );
  });
});
