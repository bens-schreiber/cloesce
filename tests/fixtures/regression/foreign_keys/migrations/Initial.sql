--- New Models
CREATE TABLE IF NOT EXISTS "B" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "Course" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "Person" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "Student" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "A" (
  "id" integer PRIMARY KEY,
  "bId" integer NOT NULL,
  FOREIGN KEY ("bId") REFERENCES "B" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "Dog" (
  "id" integer PRIMARY KEY,
  "personId" integer NOT NULL,
  FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "CourseStudent" (
  "left" integer NOT NULL,
  "right" integer NOT NULL,
  PRIMARY KEY ("left", "right"),
  FOREIGN KEY ("left") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("right") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);