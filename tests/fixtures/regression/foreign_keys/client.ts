// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";

export class A {
  id: number;
  bId: number;
  b: B | undefined;

  static async post(
        a: A,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<A>> {
    const baseUrl = new URL(`http://localhost:5002/api/A/post`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a
      })
    });
    let httpResult = HttpResult<A>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new A(), httpResult.data);
    return httpResult;
  }
  async refresh(
        __dataSource: "withB" |"withoutB" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<A>> {
    const baseUrl = new URL(`http://localhost:5002/api/A/${this.id}/refresh`);
    baseUrl.searchParams.append('__dataSource', String(__dataSource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let httpResult = HttpResult<A>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new A(), httpResult.data);
    return httpResult;
  }
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

  static async post(
        person: Person,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Person>> {
    const baseUrl = new URL(`http://localhost:5002/api/Person/post`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            person
      })
    });
    let httpResult = HttpResult<Person>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new Person(), httpResult.data);
    return httpResult;
  }
  async refresh(
        __dataSource: "withDogs" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Person>> {
    const baseUrl = new URL(`http://localhost:5002/api/Person/${this.id}/refresh`);
    baseUrl.searchParams.append('__dataSource', String(__dataSource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let httpResult = HttpResult<Person>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new Person(), httpResult.data);
    return httpResult;
  }
}
export class Student {
  id: number;
  courses: Course[];

  static async post(
        student: Student,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Student>> {
    const baseUrl = new URL(`http://localhost:5002/api/Student/post`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            student
      })
    });
    let httpResult = HttpResult<Student>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new Student(), httpResult.data);
    return httpResult;
  }
  async refresh(
        __dataSource: "withCoursesStudents" |"withCoursesStudentsCourses" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Student>> {
    const baseUrl = new URL(`http://localhost:5002/api/Student/${this.id}/refresh`);
    baseUrl.searchParams.append('__dataSource', String(__dataSource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let httpResult = HttpResult<Student>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new Student(), httpResult.data);
    return httpResult;
  }
}
