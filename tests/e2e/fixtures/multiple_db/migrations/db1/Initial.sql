--- New Models
CREATE TABLE IF NOT EXISTS "DB1Model" (
  "id" integer PRIMARY KEY,
  "someColumn" text NOT NULL
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);