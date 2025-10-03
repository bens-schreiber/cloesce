import { HttpResult, instantiateModelArray } from "cloesce";

export class A {
  id: number;
  bId: number;
  b: B | undefined;

  static async post(
        a: A,
    dataSource: "withB" | "withoutB" | null = null
  ): Promise<HttpResult<A>> {
    const baseUrl = new URL(`http://localhost:5002/api/A/post`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a
      })
    });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    raw.data = Object.assign(new A(), raw.data);
    return raw;
  }
  async refresh(
    dataSource: "withB" | "withoutB" | null = null
  ): Promise<HttpResult<A>> {
    const baseUrl = new URL(`http://localhost:5002/api/A/${this.id}/refresh`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, { method: "GET" });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    raw.data = Object.assign(new A(), raw.data);
    return raw;
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
    dataSource: "withDogs" | null = null
  ): Promise<HttpResult<Person>> {
    const baseUrl = new URL(`http://localhost:5002/api/Person/post`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            person
      })
    });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    raw.data = Object.assign(new Person(), raw.data);
    return raw;
  }
  async refresh(
    dataSource: "withDogs" | null = null
  ): Promise<HttpResult<Person>> {
    const baseUrl = new URL(`http://localhost:5002/api/Person/${this.id}/refresh`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, { method: "GET" });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    raw.data = Object.assign(new Person(), raw.data);
    return raw;
  }
}
export class Student {
  id: number;
  courses: Course[];

  static async post(
        student: Student,
    dataSource: "withCoursesStudents" | "withCoursesStudentsCourses" | null = null
  ): Promise<HttpResult<Student>> {
    const baseUrl = new URL(`http://localhost:5002/api/Student/post`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            student
      })
    });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    raw.data = Object.assign(new Student(), raw.data);
    return raw;
  }
  async refresh(
    dataSource: "withCoursesStudents" | "withCoursesStudentsCourses" | null = null
  ): Promise<HttpResult<Student>> {
    const baseUrl = new URL(`http://localhost:5002/api/Student/${this.id}/refresh`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, { method: "GET" });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    raw.data = Object.assign(new Student(), raw.data);
    return raw;
  }
}
