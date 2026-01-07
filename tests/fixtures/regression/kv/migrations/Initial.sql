--- New Models
CREATE TABLE IF NOT EXISTS "D1BackedModel" (
  "id" real PRIMARY KEY,
  "someColumn" real NOT NULL,
  "someOtherColumn" text NOT NULL
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);