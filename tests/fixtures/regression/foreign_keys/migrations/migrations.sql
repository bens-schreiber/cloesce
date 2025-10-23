-- Models
CREATE TABLE "B" ( "id" integer PRIMARY KEY );
CREATE TABLE "Course" ( "id" integer PRIMARY KEY );
CREATE TABLE "Person" ( "id" integer PRIMARY KEY );
CREATE TABLE "Student" ( "id" integer PRIMARY KEY );
CREATE TABLE "A" ( "id" integer PRIMARY KEY, "bId" integer NOT NULL, FOREIGN KEY ("bId") REFERENCES "B" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );
CREATE TABLE "Dog" ( "id" integer PRIMARY KEY, "personId" integer NOT NULL, FOREIGN KEY ("personId") REFERENCES "Person" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );
CREATE TABLE "StudentsCourses" ( "Course.id" integer NOT NULL, "Student.id" integer NOT NULL, PRIMARY KEY ("Course.id", "Student.id"), FOREIGN KEY ("Course.id") REFERENCES "Course" ("id") ON DELETE RESTRICT ON UPDATE CASCADE, FOREIGN KEY ("Student.id") REFERENCES "Student" ("id") ON DELETE RESTRICT ON UPDATE CASCADE );

-- Views / Data Sources
CREATE VIEW "A.withB" AS SELECT "A"."id" AS "A.id", "A"."bId" AS "A.bId", "B"."id" AS "A.b.id" FROM "A" LEFT JOIN "B" ON "A"."bId" = "B"."id";
CREATE VIEW "A.withoutB" AS SELECT "A"."id" AS "A.id", "A"."bId" AS "A.bId" FROM "A";
CREATE VIEW "Person.withDogs" AS SELECT "Person"."id" AS "Person.id", "Dog"."id" AS "Person.dogs.id", "Dog"."personId" AS "Person.dogs.personId" FROM "Person" LEFT JOIN "Dog" ON "Person"."id" = "Dog"."personId";
CREATE VIEW "Student.withCoursesStudents" AS SELECT "Student"."id" AS "Student.id", "StudentsCourses"."Course.id" AS "Student.courses.id", "StudentsCourses1"."Student.id" AS "Student.courses.students.id" FROM "Student" LEFT JOIN "StudentsCourses" ON "Student"."id" = "StudentsCourses"."Student.id" LEFT JOIN "Course" ON "StudentsCourses"."Course.id" = "Course"."id" LEFT JOIN "StudentsCourses" AS "StudentsCourses1" ON "Course"."id" = "StudentsCourses1"."Course.id" LEFT JOIN "Student" AS "Student1" ON "StudentsCourses1"."Student.id" = "Student1"."id";
CREATE VIEW "Student.withCoursesStudentsCourses" AS SELECT "Student"."id" AS "Student.id", "StudentsCourses"."Course.id" AS "Student.courses.id", "StudentsCourses1"."Student.id" AS "Student.courses.students.id", "StudentsCourses2"."Course.id" AS "Student.courses.students.courses.id" FROM "Student" LEFT JOIN "StudentsCourses" ON "Student"."id" = "StudentsCourses"."Student.id" LEFT JOIN "Course" ON "StudentsCourses"."Course.id" = "Course"."id" LEFT JOIN "StudentsCourses" AS "StudentsCourses1" ON "Course"."id" = "StudentsCourses1"."Course.id" LEFT JOIN "Student" AS "Student1" ON "StudentsCourses1"."Student.id" = "Student1"."id" LEFT JOIN "StudentsCourses" AS "StudentsCourses2" ON "Student1"."id" = "StudentsCourses2"."Student.id" LEFT JOIN "Course" AS "Course1" ON "StudentsCourses2"."Course.id" = "Course1"."id";

-- Cloesce
CREATE TABLE "_cloesce_tmp" ( "path" text PRIMARY KEY, "id" integer NOT NULL );