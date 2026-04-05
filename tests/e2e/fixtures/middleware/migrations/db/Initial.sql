--- New Models
CREATE TABLE IF NOT EXISTS "Foo" ("id" text PRIMARY KEY);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);