--- New Models
CREATE TABLE IF NOT EXISTS "NullabilityChecks" (
  "id" integer PRIMARY KEY,
  "notNullableString" text NOT NULL,
  "nullableString" text
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);