import {
  Model,
  POST,
  WranglerEnv,
  ForeignKey,
  GET,
  Orm,
  Inject,
  HttpResult,
  Integer,
  IncludeTree,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

//#region OneToOne
@Model()
export class B {
  id: Integer;

  @POST
  testMethod() {}
}

@Model()
export class A {
  id: Integer;

  bId: Integer;
  b: B | undefined;

  static readonly withB: IncludeTree<A> = {
    b: {},
  };

  static readonly withoutB: IncludeTree<A> = {};

  @POST
  static async post(@Inject env: Env, a: A): Promise<A> {
    const orm = Orm.fromEnv(env);
    await orm.upsert(A, a, A.withB);
    return (await orm.get(A, {
      id: a.id,
      includeTree: A.withB,
    }))!;
  }

  @POST
  static async returnFatalIfParamsNotInstantiated(
    a: A,
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
@Model()
export class Person {
  id: Integer;

  dogs: Dog[];

  static readonly withDogs: IncludeTree<Person> = {
    dogs: {},
  };

  @POST
  static async post(@Inject env: Env, person: Person): Promise<Person> {
    const orm = Orm.fromEnv(env);
    return (await orm.upsert(Person, person, Person.withDogs))!;
  }

  @POST
  static async returnFatalIfParamsNotInstantiated(
    person: Person,
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

@Model()
export class Dog {
  id: Integer;

  @ForeignKey(Person)
  personId: Integer;

  @POST
  testMethod() {}
}
//#endregion

//#region ManyToMany
@Model()
export class Student {
  id: Integer;
  courses: Course[];

  static readonly withCoursesStudents: IncludeTree<Student> = {
    courses: { students: {} },
  };

  static readonly withCoursesStudentsCourses: IncludeTree<Student> = {
    courses: { students: { courses: {} } },
  };

  @POST
  static async post(@Inject env: Env, student: Student): Promise<Student> {
    const orm = Orm.fromEnv(env);
    return (await orm.upsert(Student, student, Student.withCoursesStudents))!;
  }

  @GET
  refresh(): Student {
    return this;
  }
}

@Model()
export class Course {
  id: Integer;
  students: Student[];
}
//#endregion
