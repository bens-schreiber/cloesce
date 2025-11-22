--- New Models
CREATE TABLE IF NOT EXISTS "BlobHaver" (
  "id" real PRIMARY KEY,
  "blob1" blob NOT NULL,
  "blob2" blob NOT NULL
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);