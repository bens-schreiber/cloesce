--- New Models
CREATE TABLE IF NOT EXISTS "Horse" (
  "id" integer PRIMARY KEY,
  "name" text NOT NULL,
  "bio" text
);

CREATE TABLE IF NOT EXISTS "Like" (
  "id" integer PRIMARY KEY,
  "horse1Id" integer NOT NULL,
  "horse2Id" integer NOT NULL,
  FOREIGN KEY ("horse1Id") REFERENCES "Horse" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("horse2Id") REFERENCES "Horse" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);