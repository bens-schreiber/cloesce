--- New Models
CREATE TABLE IF NOT EXISTS "Course" (
  "id" integer PRIMARY KEY,
  "title" text NOT NULL
);

CREATE TABLE IF NOT EXISTS "Student" (
  "id" integer NOT NULL,
  "name" text NOT NULL,
  "favoriteColor" text NOT NULL,
  PRIMARY KEY ("id", "name")
);

CREATE TABLE IF NOT EXISTS "StudentCourse" (
  "studentId" integer NOT NULL,
  "studentName" text NOT NULL,
  "courseId" integer NOT NULL,
  PRIMARY KEY ("studentId", "studentName", "courseId"),
  FOREIGN KEY ("courseId") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("studentId", "studentName") REFERENCES "Student" ("id", "name") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- Cloesce Temporary Table
CREATE TABLE IF NOT EXISTS "_cloesce_tmp" (
  "path" text PRIMARY KEY,
  "primary_key" text NOT NULL
);