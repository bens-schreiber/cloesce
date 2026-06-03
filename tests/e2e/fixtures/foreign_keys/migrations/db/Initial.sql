--- New Models
CREATE TABLE IF NOT EXISTS "A" (
  "id" integer PRIMARY KEY,
  "bId" integer,
  FOREIGN KEY ("bId") REFERENCES "B" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "B" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "Course" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "Person" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "Student" ("id" integer PRIMARY KEY);

CREATE TABLE IF NOT EXISTS "Dog" (
  "id" integer PRIMARY KEY,
  "personId" integer NOT NULL,
  FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS "CourseStudent" (
  "courseId" integer NOT NULL,
  "studentId" integer NOT NULL,
  PRIMARY KEY ("courseId", "studentId"),
  FOREIGN KEY ("courseId") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("studentId") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "$cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);