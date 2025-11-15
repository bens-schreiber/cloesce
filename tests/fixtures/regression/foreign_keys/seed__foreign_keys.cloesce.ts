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
  Orm,
  Inject,
  HttpResult,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";
class HttpResult<T = unknown> {}
type Integer = number & { __kind: "Integer" };

@WranglerEnv
export class Env {
  db: D1Database;
}

//#region OneToOne
@D1
export class B {
  @PrimaryKey
  id: Integer;

  @POST
  testMethod() {}
}

@D1
export class A {
  @PrimaryKey
  id: Integer;

  @ForeignKey(B)
  bId: Integer;

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
    await orm.upsert(A, a, A.withB);
    return (await orm.get(A, a.id, A.withB)).value;
  }

  @POST
  static async returnFatalIfParamsNotInstantiated(
    a: A
  ): Promise<HttpResult<void>> {
    if (!a.refresh) {
      return HttpResult.fail(500, "a.refresh was undefined");
    }

    if (!a.b?.testMethod) {
      return HttpResult.fail(500, "a.b was undefined");
    }

    return HttpResult.ok(200);
  }

  @GET
  refresh(): A {
    return this;
  }
}

//#endregion

//#region OneToMany
@D1
export class Person {
  @PrimaryKey
  id: Integer;

  @OneToMany("personId")
  dogs: Dog[];

  @DataSource
  static readonly withDogs: IncludeTree<Person> = {
    dogs: {},
  };

  @POST
  static async post(@Inject { db }: Env, person: Person): Promise<Person> {
    const orm = Orm.fromD1(db);
    await orm.upsert(Person, person, Person.withDogs);
    return (await orm.get(Person, person.id, Person.withDogs)).value;
  }

  @POST
  static async returnFatalIfParamsNotInstantiated(
    person: Person
  ): Promise<HttpResult<void>> {
    if (person.refresh === undefined) {
      return HttpResult.fail(500);
    }

    if (person.dogs === undefined) {
      return HttpResult.fail(500);
    }

    if (person.dogs.some((d) => d.testMethod === undefined)) {
      return HttpResult.fail(500);
    }

    return HttpResult.ok(200);
  }

  @GET
  refresh(): Person {
    return this;
  }
}

@D1
export class Dog {
  @PrimaryKey
  id: Integer;

  @ForeignKey(Person)
  personId: Integer;

  @POST
  testMethod() {}
}
//#endregion

//#region ManyToMany
@D1
export class Student {
  @PrimaryKey
  id: Integer;

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
    await orm.upsert(Student, student, Student.withCoursesStudents);
    return student;
  }

  @GET
  refresh(): Student {
    return this;
  }
}

@D1
export class Course {
  @PrimaryKey
  id: Integer;

  @ManyToMany("StudentsCourses")
  students: Student[];
}
//#endregion
