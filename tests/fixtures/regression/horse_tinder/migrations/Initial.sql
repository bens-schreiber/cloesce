--- New Models
CREATE TABLE IF NOT EXISTS "Horse" (
  "id" integer PRIMARY KEY,
  "name" text NOT NULL,
  "bio" text
);

CREATE TABLE IF NOT EXISTS "Like" (
  "id" integer PRIMARY KEY,
  "horseId1" integer NOT NULL,
  "horseId2" integer NOT NULL,
  FOREIGN KEY ("horseId1") REFERENCES "Horse" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("horseId2") REFERENCES "Horse" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);