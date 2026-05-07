import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, withRes } from "../src/setup";
import { DB1Model, DB2Model } from "../fixtures/multiple_db/client";
import config from "../fixtures/multiple_db/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/multiple_db", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Multiple DBs", () => {
  it("CRUD on DB1Model", async () => {
    const save = await DB1Model.$save({
      someColumn: "test",
    });
    expect(save.ok, withRes("POST should be OK", save)).toBe(true);
    expect(save.data).toEqual({
      id: 1,
      someColumn: "test",
    });

    const $get = await DB1Model.$get(save.data!.id);
    expect($get.ok, withRes("$get should be OK", $get)).toBe(true);
    expect($get.data).toEqual({
      id: 1,
      someColumn: "test",
    });

    const list = await DB1Model.$list(0, 10);

    expect(list.ok, withRes("$list should be OK", list)).toBe(true);
    expect(list.data).toEqual([
      {
        id: 1,
        someColumn: "test",
      },
    ]);
  });

  it("CRUD on DB2Model", async () => {
    const save = await DB2Model.$save({
      someColumn: "test2",
    });
    expect(save.ok, withRes("POST should be OK", save)).toBe(true);
    expect(save.data).toEqual({
      id: 1,
      someColumn: "test2",
    });

    const get = await DB2Model.$get(save.data!.id);
    expect(get.ok, withRes("$get should be OK", get)).toBe(true);
    expect(get.data).toEqual({
      id: 1,
      someColumn: "test2",
    });

    const list = await DB2Model.$list(0, 10);
    expect(list.ok, withRes("$list should be OK", list)).toBe(true);
    expect(list.data).toEqual([
      {
        id: 1,
        someColumn: "test2",
      },
    ]);
  });
});
