--- New Models
CREATE TABLE IF NOT EXISTS "Weather" (
  "id" integer PRIMARY KEY,
  "date" text NOT NULL,
  "isRaining" integer NOT NULL
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);