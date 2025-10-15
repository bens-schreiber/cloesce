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
  Inject,
  Orm,
} from "cloesce/backend";

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
    const orm = Orm.fromD1(db);
    await orm.insert(A, a, A.withB);
    return (await orm.get(A, a.id, "withB")).value;
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
    const orm = Orm.fromD1(db);
    await orm.insert(Person, person, Person.withDogs);
    return (await orm.get(Person, person.id, "withDogs")).value;
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

  @ForeignKey(Person)
  personId2: number;
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
    const orm = Orm.fromD1(db);
    await orm.insert(Student, student, Student.withCoursesStudents);
    return student;
    // return (await orm.get(Student, student.id, "withCoursesStudents")).value;
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
