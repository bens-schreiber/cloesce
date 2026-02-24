--- New Models
CREATE TABLE IF NOT EXISTS "Hamburger" ("id" integer PRIMARY KEY, "name" text NOT NULL);

CREATE TABLE IF NOT EXISTS "Topping" ("id" integer PRIMARY KEY, "name" text NOT NULL);

CREATE TABLE IF NOT EXISTS "HamburgerTopping" (
  "left" integer NOT NULL,
  "right" integer NOT NULL,
  PRIMARY KEY ("left", "right"),
  FOREIGN KEY ("left") REFERENCES "Hamburger" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("right") REFERENCES "Topping" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);