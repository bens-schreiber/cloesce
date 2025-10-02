import { HttpResult, instantiateModelArray } from "cloesce";

export class A {
  id: number;
  bId: number;
  b: B | undefined;

}
export class B {
  id: number;

}
export class Course {
  id: number;
  students: Student[];

}
export class Dog {
  id: number;
  personId: number;

}
export class Person {
  id: number;
  dogs: Dog[];

}
export class Student {
  id: number;
  courses: Course[];

}
