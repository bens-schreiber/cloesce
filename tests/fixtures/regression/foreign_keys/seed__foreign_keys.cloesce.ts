import {
  Model,
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
  HttpResult as CloesceHttpResult,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";
type Integer = number & { __kind: "Integer" };
class HttpResult<T = unknown> { }

@WranglerEnv
export class Env {
  db: D1Database;
}

//#region OneToOne
@Model
export class B {
  @PrimaryKey
  id: Integer;

  @POST
  testMethod() { }
}

@Model
export class A {
  @PrimaryKey
  id: Integer;

  @ForeignKey(B)
  bId: Integer;

  @OneToOne("bId")
  b: B | undefined;

  static readonly withB: IncludeTree<A> = {
    b: {},
  };

  static readonly withoutB: IncludeTree<A> = {};

  @POST
  static async post(@Inject env: Env, a: A): Promise<A> {
    const orm = Orm.fromEnv(env);
    await orm.upsert(A, a, A.withB);
    return await orm.get(A, {
      id: a.id,
      includeTree: A.withB,
    });
  }

  @POST
  static async returnFatalIfParamsNotInstantiated(
    a: A
  ): Promise<HttpResult<void>> {
    if (!a.refresh) {
      return CloesceHttpResult.fail(500, "a.refresh was undefined");
    }

    if (!a.b?.testMethod) {
      return CloesceHttpResult.fail(500, "a.b was undefined");
    }

    return CloesceHttpResult.ok(200);
  }

  @GET
  refresh(): A {
    return this;
  }
}

//#endregion

//#region OneToMany
@Model
export class Person {
  @PrimaryKey
  id: Integer;

  @OneToMany("personId")
  dogs: Dog[];

  static readonly withDogs: IncludeTree<Person> = {
    dogs: {},
  };

  @POST
  static async post(@Inject env: Env, person: Person): Promise<Person> {
    const orm = Orm.fromEnv(env);
    return await orm.upsert(Person, person, Person.withDogs);
  }

  @POST
  static async returnFatalIfParamsNotInstantiated(
    person: Person
  ): Promise<HttpResult<void>> {
    if (person.refresh === undefined) {
      return CloesceHttpResult.fail(500);
    }

    if (person.dogs === undefined) {
      return CloesceHttpResult.fail(500);
    }

    if (person.dogs.some((d) => d.testMethod === undefined)) {
      return CloesceHttpResult.fail(500);
    }

    return CloesceHttpResult.ok(200);
  }

  @GET
  refresh(): Person {
    return this;
  }
}

@Model
export class Dog {
  @PrimaryKey
  id: Integer;

  @ForeignKey(Person)
  personId: Integer;

  @POST
  testMethod() { }
}
//#endregion

//#region ManyToMany
@Model
export class Student {
  @PrimaryKey
  id: Integer;

  @ManyToMany
  courses: Course[];

  static readonly withCoursesStudents: IncludeTree<Student> = {
    courses: { students: {} },
  };

  static readonly withCoursesStudentsCourses: IncludeTree<Student> =
    {
      courses: { students: { courses: {} } },
    };

  @POST
  static async post(@Inject env: Env, student: Student): Promise<Student> {
    const orm = Orm.fromEnv(env);
    return await orm.upsert(Student, student, Student.withCoursesStudents);
  }

  @GET
  refresh(): Student {
    return this;
  }
}

@Model
export class Course {
  @PrimaryKey
  id: Integer;

  @ManyToMany
  students: Student[];
}
//#endregion
