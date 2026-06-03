--- New Models
CREATE TABLE IF NOT EXISTS "DefaultOverride" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "Hamburger" ("id" integer PRIMARY KEY, "name" text NOT NULL);

CREATE TABLE IF NOT EXISTS "Topping" ("id" integer PRIMARY KEY, "name" text NOT NULL);

CREATE TABLE IF NOT EXISTS "HamburgerTopping" (
  "hamburgerId" integer NOT NULL,
  "toppingId" integer NOT NULL,
  PRIMARY KEY ("hamburgerId", "toppingId"),
  FOREIGN KEY ("hamburgerId") REFERENCES "Hamburger" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("toppingId") REFERENCES "Topping" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "$cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);