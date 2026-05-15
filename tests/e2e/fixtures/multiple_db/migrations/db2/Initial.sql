--- New Models
CREATE TABLE IF NOT EXISTS "DB2Model" (
  "id" integer PRIMARY KEY,
  "someColumn" text NOT NULL
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "$cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);