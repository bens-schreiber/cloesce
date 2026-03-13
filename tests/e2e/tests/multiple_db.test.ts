import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, withRes } from "../src/setup";
import { DB1Model, DB2Model } from "../fixtures/multiple_db/client";
import config from "../fixtures/multiple_db/cloesce.config";

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler(
    "./fixtures/multiple_db",
    config.workersUrl!,
  );
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Multiple DBs", () => {
  it("CRUD on DB1Model", async () => {
    const save = await DB1Model.SAVE({
      someColumn: "test",
    });
    expect(save.ok, withRes("POST should be OK", save)).toBe(true);
    expect(save.data).toEqual({
      id: 1,
      someColumn: "test",
    });

    const get = await DB1Model.GET(save.data!.id);
    expect(get.ok, withRes("GET should be OK", get)).toBe(true);
    expect(get.data).toEqual({
      id: 1,
      someColumn: "test",
    });

    const list = await DB1Model.LIST(null, null, null);
    expect(list.ok, withRes("LIST should be OK", list)).toBe(true);
    expect(list.data).toEqual([
      {
        id: 1,
        someColumn: "test",
      },
    ]);
  });

  it("CRUD on DB2Model", async () => {
    const save = await DB2Model.SAVE({
      someColumn: "test2",
    });
    expect(save.ok, withRes("POST should be OK", save)).toBe(true);
    expect(save.data).toEqual({
      id: 1,
      someColumn: "test2",
    });

    const get = await DB2Model.GET(save.data!.id);
    expect(get.ok, withRes("GET should be OK", get)).toBe(true);
    expect(get.data).toEqual({
      id: 1,
      someColumn: "test2",
    });

    const list = await DB2Model.LIST(null, null, null);
    expect(list.ok, withRes("LIST should be OK", list)).toBe(true);
    expect(list.data).toEqual([
      {
        id: 1,
        someColumn: "test2",
      },
    ]);
  });
});
