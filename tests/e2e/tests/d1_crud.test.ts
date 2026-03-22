import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, withRes } from "../src/setup";
import { CrudHaver, Parent } from "../fixtures/d1_crud/client";
import config from "../fixtures/d1_crud/cloesce.config";

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/d1_crud", config.workersUrl!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Basic", () => {
  let model: CrudHaver;
  it("POST", async () => {
    const res = await CrudHaver.SAVE({
      name: "tim",
    });
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data).toEqual({
      id: 1,
      name: "tim",
    });
    model = res.data!;
  });

  it("POST Update", async () => {
    model.name = "julio";
    const res = await CrudHaver.SAVE(model);
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data).toEqual({
      id: 1,
      name: "julio",
    });
    model = res.data!;
  });

  it("GET a model", async () => {
    const res = await CrudHaver.GET(1);
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toEqual(model);
  });

  const models = ["a", "b", "c"];
  it("POST 3 Models", async () => {
    await Promise.all(
      models.map(async (m) => {
        const res = await CrudHaver.SAVE({ name: m });
        expect(res.ok, withRes("POST should be OK", res)).toBe(true);
        expect(res.data!.name).toEqual(m);
      }),
    );
  });

  it("List 3 models", async () => {
    const res = await CrudHaver.LIST(null, null, null);
    expect(res.ok, withRes("LIST should be OK", res)).toBe(true);
    expect(res.data!.length, withRes("Should be 4 results", res)).toBe(4); // including the one from the prev test
    models.forEach((m) =>
      expect(res.data!.map((d: CrudHaver) => d.name)).toContain(m),
    );
  });
});

describe("Parent with children", () => {
  let model: Parent;
  it("POST", async () => {
    const res = await Parent.SAVE(
      {
        // leaving out favoriteChildId, should infer as null
        children: [{}, {}, {}], // should be able to leave blank, creating 3 children
      },
      "withChildren",
    );

    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data, withRes("Data should be equal", res)).toEqual({
      id: 1,
      favoriteChildId: null,
      children: [
        { id: 1, parentId: 1 },
        { id: 2, parentId: 1 },
        { id: 3, parentId: 1 },
      ],
    });

    model = res.data!;
  });

  it("POST Update", async () => {
    model.favoriteChildId = model.children[0].id;
    const res = await Parent.SAVE(model, "withChildren");

    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data, withRes("Data should be equal", res)).toEqual({
      id: 1,
      favoriteChildId: 1,
      favoriteChild: {
        id: 1,
        parentId: 1,
      },
      children: [
        { id: 1, parentId: 1 },
        { id: 2, parentId: 1 },
        { id: 3, parentId: 1 },
      ],
    });

    model = res.data!;
  });

  it("GET", async () => {
    const res = await Parent.GET(1, "withChildren");
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data, withRes("Data should be equal", res)).toEqual(model);
  });

  it("LIST", async () => {
    const res = await Parent.LIST(null, null, null, "withChildren");
    expect(res.ok, withRes("LIST should be OK", res)).toBe(true);
    expect(res.data!.length).toEqual(1);
    expect(res.data![0], withRes("Data should be equal", res)).toEqual(model);
  });
});
