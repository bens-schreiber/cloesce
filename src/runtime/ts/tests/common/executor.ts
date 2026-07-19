import { expect } from "vitest";
import type { KeyStore, SqlStore, StorageResolver } from "../../src/router/executor/index.js";
import type { Database } from "../../src/router/executor/plan.js";

export type QueryCall = { sql: string; bindings: unknown[] };
export type BatchCall = { statements: QueryCall[] };

export class MockSqlStore implements SqlStore {
  queries: QueryCall[] = [];
  batches: BatchCall[] = [];

  constructor(
    private queryResponder: (call: QueryCall) => Record<string, unknown>[] = () => [],
    private batchResponder: (call: BatchCall) => Record<string, unknown>[][] = () => [],
  ) {}

  async query(sql: string, bindings: unknown[]): Promise<Record<string, unknown>[]> {
    const call = { sql, bindings };
    this.queries.push(call);
    return this.queryResponder(call);
  }

  async batch(statements: QueryCall[]): Promise<Record<string, unknown>[][]> {
    const call = { statements };
    this.batches.push(call);
    return this.batchResponder(call);
  }
}

export class MockKeyStore implements KeyStore {
  gets: string[] = [];
  puts: { key: string; value: unknown; metadata: unknown }[] = [];

  constructor(private store: Map<string, unknown> = new Map()) {}

  get(key: string): unknown {
    this.gets.push(key);
    return this.store.has(key) ? this.store.get(key) : null;
  }

  put(key: string, value: unknown, metadata?: unknown): void {
    this.puts.push({ key, value, metadata });
    this.store.set(key, value);
  }
}

export class MockResolver implements StorageResolver {
  sqlStores = new Map<string, MockSqlStore>();
  keyStores = new Map<string, MockKeyStore>();

  constructor(
    private sqlFactory: (database: Database, shard: unknown[]) => MockSqlStore = () =>
      new MockSqlStore(),
    private keyFactory: (database: Database, shard: unknown[]) => MockKeyStore = () =>
      new MockKeyStore(),
  ) {}

  private slot(database: Database, shard: unknown[]): string {
    return `${database.name}|${JSON.stringify(shard)}`;
  }

  sql(database: Database, shard: unknown[]): SqlStore {
    const slot = this.slot(database, shard);
    if (!this.sqlStores.has(slot)) {
      this.sqlStores.set(slot, this.sqlFactory(database, shard));
    }
    return this.sqlStores.get(slot)!;
  }

  key(database: Database, shard: unknown[]): KeyStore {
    const slot = this.slot(database, shard);
    if (!this.keyStores.has(slot)) {
      this.keyStores.set(slot, this.keyFactory(database, shard));
    }
    return this.keyStores.get(slot)!;
  }
}

export function d1(name = "db"): Database {
  return { name, kind: "D1" };
}
export function doDb(name = "doDb"): Database {
  return { name, kind: "DurableObject" };
}
export function kvDb(name = "kv"): Database {
  return { name, kind: "Kv" };
}
export function r2Db(name = "r2"): Database {
  return { name, kind: "R2" };
}

/** Match a sunk error of `kind` whose Error message matches `message`. */
export function sunkError(kind: string, message: RegExp) {
  return expect.objectContaining({
    kind,
    error: expect.objectContaining({ message: expect.stringMatching(message) }),
  });
}
