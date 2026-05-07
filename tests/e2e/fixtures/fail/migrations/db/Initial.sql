--- New Models
CREATE TABLE IF NOT EXISTS "FailModel" ("id" integer PRIMARY KEY, "name" text NOT NULL);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);