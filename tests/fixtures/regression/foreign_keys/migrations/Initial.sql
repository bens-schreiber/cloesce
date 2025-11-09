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

CREATE TABLE IF NOT EXISTS "StudentsCourses" (
  "Student.id" integer NOT NULL,
  "Course.id" integer NOT NULL,
  PRIMARY KEY ("Student.id", "Course.id"),
  FOREIGN KEY ("Student.id") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE,
  FOREIGN KEY ("Course.id") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE
);

--- New Data Sources
CREATE VIEW IF NOT EXISTS "Person.withDogs" AS
SELECT
  "Person"."id" AS "id",
  "Dog"."id" AS "dogs.id",
  "Dog"."personId" AS "dogs.personId"
FROM
  "Person"
  LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId";

CREATE VIEW IF NOT EXISTS "Student.withCoursesStudents" AS
SELECT
  "Student"."id" AS "id",
  "StudentsCourses"."Course.id" AS "courses.id",
  "StudentsCourses1"."Student.id" AS "courses.students.id"
FROM
  "Student"
  LEFT JOIN "StudentsCourses" ON "Student"."id" = "StudentsCourses"."Student.id"
  LEFT JOIN "Course" ON "StudentsCourses"."Course.id" = "Course"."id"
  LEFT JOIN "StudentsCourses" AS "StudentsCourses1" ON "Course"."id" = "StudentsCourses1"."Course.id"
  LEFT JOIN "Student" AS "Student1" ON "StudentsCourses1"."Student.id" = "Student1"."id";

CREATE VIEW IF NOT EXISTS "Student.withCoursesStudentsCourses" AS
SELECT
  "Student"."id" AS "id",
  "StudentsCourses"."Course.id" AS "courses.id",
  "StudentsCourses1"."Student.id" AS "courses.students.id",
  "StudentsCourses2"."Course.id" AS "courses.students.courses.id"
FROM
  "Student"
  LEFT JOIN "StudentsCourses" ON "Student"."id" = "StudentsCourses"."Student.id"
  LEFT JOIN "Course" ON "StudentsCourses"."Course.id" = "Course"."id"
  LEFT JOIN "StudentsCourses" AS "StudentsCourses1" ON "Course"."id" = "StudentsCourses1"."Course.id"
  LEFT JOIN "Student" AS "Student1" ON "StudentsCourses1"."Student.id" = "Student1"."id"
  LEFT JOIN "StudentsCourses" AS "StudentsCourses2" ON "Student1"."id" = "StudentsCourses2"."Student.id"
  LEFT JOIN "Course" AS "Course1" ON "StudentsCourses2"."Course.id" = "Course1"."id";

CREATE VIEW IF NOT EXISTS "A.withB" AS
SELECT
  "A"."id" AS "id",
  "A"."bId" AS "bId",
  "B"."id" AS "b.id"
FROM
  "A"
  LEFT JOIN "B" ON "A"."bId" = "B"."id";

CREATE VIEW IF NOT EXISTS "A.withoutB" AS
SELECT
  "A"."id" AS "id",
  "A"."bId" AS "bId"
FROM
  "A";

--- Cloesce Temporary Table
CREATE TABLE "_cloesce_tmp" ("path" text PRIMARY KEY, "id" integer NOT NULL);