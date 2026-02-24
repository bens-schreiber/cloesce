import {
  Model,
  Post,
  WranglerEnv,
  ForeignKey,
  Get,
  Orm,
  Inject,
  HttpResult,
  Integer,
  DataSource,
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

  @Post()
  testMethod() { }
}

@Model()
export class A {
  id: Integer;

  bId: Integer;
  b: B | undefined;

  static readonly withB: DataSource<A> = {
    includeTree: {
      b: {},
    },
  };

  static readonly withoutB: DataSource<A> = {
    includeTree: {},
  };

  @Post()
  static async post(@Inject env: Env, a: A): Promise<A> {
    const orm = Orm.fromEnv(env);
    await orm.upsert(A, a, A.withB);
    return (await orm.get(A, {
      id: a.id,
      include: A.withB,
    }))!;
  }

  @Post()
  static async returnFatalIfParamsNotInstantiated(
    a: A,
  ): Promise<HttpResult<void>> {
    if (!a.b?.testMethod) {
      return HttpResult.fail(500, "a.b was undefined");
    }

    return HttpResult.ok(200);
  }

  @Get({ includeTree: {} })
  async withoutB(): Promise<A> {
    return this;
  }
}

//#endregion

//#region OneToMany
@Model()
export class Person {
  id: Integer;

  dogs: Dog[];

  static readonly withDogs: DataSource<Person> = {
    includeTree: {
      dogs: {},
    },
  };

  @Post()
  static async post(@Inject env: Env, person: Person): Promise<Person> {
    const orm = Orm.fromEnv(env);
    return (await orm.upsert(Person, person, Person.withDogs))!;
  }

  @Post()
  static async returnFatalIfParamsNotInstantiated(
    person: Person,
  ): Promise<HttpResult<void>> {
    if (person.dogs === undefined) {
      return HttpResult.fail(500);
    }

    if (person.dogs.some((d) => d.testMethod === undefined)) {
      return HttpResult.fail(500);
    }

    return HttpResult.ok(200);
  }

  @Get({ includeTree: {} })
  async withoutDogs(): Promise<Person> {
    return this;
  }
}

@Model()
export class Dog {
  id: Integer;

  @ForeignKey(Person)
  personId: Integer;

  @Post()
  testMethod() { }
}
//#endregion

//#region ManyToMany
@Model()
export class Student {
  id: Integer;
  courses: Course[];

  static readonly withCoursesStudentsCourses: DataSource<Student> = {
    includeTree: {
      courses: { students: { courses: {} } },
    },
  };

  @Post()
  static async post(@Inject env: Env, student: Student): Promise<Student> {
    const orm = Orm.fromEnv(env);
    return (await orm.upsert(Student, student, Student.withCoursesStudentsCourses))!;
  }

  @Get({ includeTree: {} })
  async none(): Promise<Student> {
    return this;
  }
}

@Model()
export class Course {
  id: Integer;
  students: Student[];
}
//#endregion
