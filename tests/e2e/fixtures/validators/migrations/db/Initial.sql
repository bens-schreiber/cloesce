--- New Models
CREATE TABLE IF NOT EXISTS "Validator" (
  "id" integer PRIMARY KEY,
  "email" text NOT NULL
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);