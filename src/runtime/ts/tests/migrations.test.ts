import { describe, test, expect } from "vitest";
import type { SqlStorage } from "@cloudflare/workers-types";
import { applyDurableMigrations, DurableMigration } from "../src/migrations.js";

function mockSqlStorage() {
  const executed: { query: string; bindings: any[] }[] = [];
  const migrationRows: { id: string; applied_at: number }[] = [];

  const sql = {
    exec(query: string, ...bindings: any[]) {
      executed.push({ query, bindings });
      if (query.trimStart().startsWith(`SELECT id FROM "$cloesce_migrations"`)) {
        return { toArray: () => migrationRows.map((r) => ({ ...r })) };
      }
      if (query.trimStart().startsWith(`INSERT INTO "$cloesce_migrations"`)) {
        migrationRows.push({ id: bindings[0], applied_at: bindings[1] });
      }
      return { toArray: () => [] };
    },
  } as unknown as SqlStorage;

  return { sql, executed, migrationRows };
}

function migration(name: string, timestamp: number, ran: string[]): DurableMigration {
  return {
    name,
    timestamp,
    id: `${name}_${timestamp}`,
    up: () => {
      ran.push(`${name}_${timestamp}`);
    },
  };
}

describe("applyDurableMigrations", () => {
  test("applies pending migrations in timestamp order and records them", async () => {
    const storage = mockSqlStorage();
    const ran: string[] = [];

    await applyDurableMigrations({ sql: storage.sql }, [
      migration("second", 200, ran),
      migration("first", 100, ran),
    ]);

    expect(ran).toEqual(["first_100", "second_200"]);
    expect(storage.migrationRows.map((r) => r.id)).toEqual(["first_100", "second_200"]);
  });

  test("a migration runs exactly once across repeated calls", async () => {
    const storage = mockSqlStorage();
    const ran: string[] = [];
    const migrations = [migration("init", 100, ran)];

    await applyDurableMigrations({ sql: storage.sql }, migrations);
    await applyDurableMigrations({ sql: storage.sql }, [
      ...migrations,
      migration("addColumn", 200, ran),
    ]);

    expect(ran).toEqual(["init_100", "addColumn_200"]);
    expect(storage.migrationRows.map((r) => r.id)).toEqual(["init_100", "addColumn_200"]);
  });

  test("no migrations touches no storage", async () => {
    const storage = mockSqlStorage();

    await applyDurableMigrations({ sql: storage.sql }, []);

    expect(storage.executed).toEqual([]);
  });

  test("a failing migration is not recorded as applied", async () => {
    const storage = mockSqlStorage();
    const failing: DurableMigration = {
      name: "broken",
      timestamp: 100,
      id: "broken_100",
      up: () => {
        throw new Error("nope");
      },
    };

    await expect(applyDurableMigrations({ sql: storage.sql }, [failing])).rejects.toThrow("nope");
    expect(storage.migrationRows).toEqual([]);

    // It is retried on the next run.
    const ran: string[] = [];
    await applyDurableMigrations({ sql: storage.sql }, [migration("broken", 100, ran)]);
    expect(ran).toEqual(["broken_100"]);
  });
});
