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
} from "cloesce";

type D1Database = {};

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
  static readonly default: IncludeTree<A> = {
    b: {},
  };
}

//#endregion

//#region One to Many
@D1
export class Person {
  @PrimaryKey
  id: number;

  @OneToMany("personId")
  dogs: Dog[];

  @DataSource
  static readonly default: IncludeTree<Person> = {
    dogs: {},
  };
}

@D1
export class Dog {
  @PrimaryKey
  id: number;

  @ForeignKey(Person)
  personId: number;
}
//#endregion

//#region Many To Many
@D1
export class Student {
  @PrimaryKey
  id: number;

  @ManyToMany("StudentsCourses")
  courses: Course[];

  @DataSource
  static readonly default: IncludeTree<Student> = {
    courses: {
      students: {},
    },
  };
}

@D1
export class Course {
  @PrimaryKey
  id: number;

  @ManyToMany("StudentsCourses")
  students: Student[];

  @DataSource
  static readonly default: IncludeTree<Course> = {
    students: {
      courses: {},
    },
  };
}
//#endregion
