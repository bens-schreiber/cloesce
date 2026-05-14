--- New Models
CREATE TABLE IF NOT EXISTS "BlobHaver" (
  "id" integer PRIMARY KEY,
  "blob1" blob NOT NULL,
  "blob2" blob NOT NULL
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "$cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);