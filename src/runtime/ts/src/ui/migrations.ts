import type { SqlStorage } from "@cloudflare/workers-types";

/**
 * A migration of a Durable Object's SQLite database.
 *
 * Migrations cannot be applied to a Durable Object from the outside (there is no
 * Wrangler CLI command, unlike D1), so Cloesce generates them as code that runs on
 * the DO instance itself. The generated `<binding>/<timestamp>_<name>.ts` files
 * default-export this shape.
 */
export interface DurableMigration {
  name: string;
  timestamp: number;
  id: string;
  up(sql: SqlStorage): void | Promise<void>;
}

const MIGRATIONS_TABLE = "$cloesce_migrations";

/**
 * Applies all pending migrations to a Durable Object's SQLite storage, in timestamp
 * order. Applied migration ids are tracked in a `$cloesce_migrations` table so a
 * migration runs exactly once per DO instance.
 *
 * Call from a Durable Object's constructor inside `blockConcurrencyWhile` (the
 * generated `cloesce(env, migrations)` method does this for you).
 */
export async function applyDurableMigrations(
  storage: { sql: SqlStorage },
  migrations: DurableMigration[],
): Promise<void> {
  if (migrations.length === 0) {
    return;
  }

  storage.sql.exec(
    `CREATE TABLE IF NOT EXISTS "${MIGRATIONS_TABLE}" (id TEXT PRIMARY KEY, applied_at INTEGER NOT NULL);`,
  );
  const applied = new Set(
    storage.sql
      .exec(`SELECT id FROM "${MIGRATIONS_TABLE}";`)
      .toArray()
      .map((row) => row.id as string),
  );

  const pending = migrations
    .filter((m) => !applied.has(m.id))
    .sort((a, b) => a.timestamp - b.timestamp);

  for (const migration of pending) {
    await migration.up(storage.sql);
    storage.sql.exec(
      `INSERT INTO "${MIGRATIONS_TABLE}" (id, applied_at) VALUES (?, ?);`,
      migration.id,
      Date.now(),
    );
  }
}
