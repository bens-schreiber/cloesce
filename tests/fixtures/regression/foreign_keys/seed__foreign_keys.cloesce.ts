import {
  D1,
  POST,
  PrimaryKey,
  WranglerEnv,
  ForeignKey,
  OneToOne,
  DataSource,
  OneToMany,
  ManyToMany,
  IncludeTree,
  GET,
  HttpResult,
  Inject,
  modelsFromSql,
} from "cloesce";

import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

//#region OneToOne
@D1
export class B {
  @PrimaryKey
  id: number;
}

@D1
export class A {
  @PrimaryKey
  id: number;

  @ForeignKey(B)
  bId: number;

  @OneToOne("bId")
  b: B | undefined;

  @DataSource
  static readonly withB: IncludeTree<A> = {
    b: {},
  };

  @DataSource
  static readonly withoutB: IncludeTree<A> = {};

  @POST
  static async post(@Inject { db }: Env, a: A): Promise<A> {
    // Insert B
    let b;
    if (a.bId) {
      const bRecords = await db
        .prepare("INSERT INTO B (id) VALUES (?) RETURNING *")
        .bind(a.bId)
        .all();

      b = modelsFromSql(B, bRecords.results, null)[0] as B;
    }

    // Insert A
    const records = await db
      .prepare("INSERT INTO A (id, bId) VALUES (?, ?) RETURNING *")
      .bind(a.id, a.bId)
      .all();

    let resultA = modelsFromSql(A, records.results, null)[0] as A;
    resultA.b = b;

    return resultA;
  }

  @GET
  async refresh(): Promise<A> {
    return this;
  }
}

//#endregion

//#region OneToMany
@D1
export class Person {
  @PrimaryKey
  id: number;

  @OneToMany("personId")
  dogs: Dog[];

  @DataSource
  static readonly withDogs: IncludeTree<Person> = {
    dogs: {},
  };

  @POST
  static async post(@Inject { db }: Env, person: Person): Promise<Person> {
    // Insert Person
    const records = await db
      .prepare("INSERT INTO Person (id) VALUES (?) RETURNING *")
      .bind(person.id)
      .all();

    let resultPerson = modelsFromSql(
      Person,
      records.results,
      null
    )[0] as Person;

    // Insert Dogs if provided
    if (person.dogs?.length) {
      for (const dog of person.dogs) {
        await db
          .prepare("INSERT INTO Dog (id, personId) VALUES (?, ?)")
          .bind(dog.id, resultPerson.id)
          .run();
      }

      // Attach the inserted dogs
      resultPerson.dogs = person.dogs.map((d) => ({
        ...d,
        personId: resultPerson.id,
      }));
    } else {
      resultPerson.dogs = [];
    }

    return resultPerson;
  }

  @GET
  async refresh(): Promise<Person> {
    return this;
  }
}

@D1
export class Dog {
  @PrimaryKey
  id: number;

  @ForeignKey(Person)
  personId: number;
}
//#endregion

//#region ManyToMany
@D1
export class Student {
  @PrimaryKey
  id: number;

  @ManyToMany("StudentsCourses")
  courses: Course[];

  @DataSource static readonly withCoursesStudents: IncludeTree<Student> = {
    courses: { students: {} },
  };

  @DataSource static readonly withCoursesStudentsCourses: IncludeTree<Student> =
    {
      courses: { students: { courses: {} } },
    };

  @POST
  static async post(@Inject { db }: Env, student: Student): Promise<Student> {
    // Insert Student
    const records = await db
      .prepare("INSERT INTO Student (id) VALUES (?) RETURNING *")
      .bind(student.id)
      .all();

    let resultStudent = modelsFromSql(
      Student,
      records.results,
      null
    )[0] as Student;

    // Insert Courses and the join table if courses provided
    if (student.courses?.length) {
      for (const course of student.courses) {
        // Insert course if not already existing
        await db
          .prepare("INSERT OR IGNORE INTO Course (id) VALUES (?)")
          .bind(course.id)
          .run();

        // Insert into join table
        await db
          .prepare(
            "INSERT INTO StudentsCourses ([Student.id], [Course.id]) VALUES (?, ?)"
          )
          .bind(resultStudent.id, course.id)
          .run();
      }

      resultStudent.courses = student.courses;
    } else {
      resultStudent.courses = [];
    }

    return resultStudent;
  }

  @GET
  async refresh(): Promise<Student> {
    return this;
  }
}

@D1
export class Course {
  @PrimaryKey
  id: number;

  @ManyToMany("StudentsCourses")
  students: Student[];
}
//#endregion
