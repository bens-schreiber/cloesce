--- New Models
CREATE TABLE IF NOT EXISTS "CrudHaver" ("id" integer PRIMARY KEY, "name" text NOT NULL);

CREATE TABLE IF NOT EXISTS "Parent" (
  "id" integer PRIMARY KEY,
  "favoriteChildId" integer,
  FOREIGN KEY ("favoriteChildId") REFERENCES "Child" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "Child" (
  "id" integer PRIMARY KEY,
  "parentId" integer NOT NULL,
  FOREIGN KEY ("parentId") REFERENCES "Parent" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);