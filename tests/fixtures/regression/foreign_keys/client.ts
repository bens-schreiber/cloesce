// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType } from "cloesce/client";


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
      body: JSON.stringify({
            a
      })
    });

    return await HttpResult.fromResponse<A>(
      res, 
      MediaType.Json,
      A, false
    );
  }
  async refresh(
    __dataSource: "withB" |"withoutB" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<A>> {
    const baseUrl = new URL(`http://localhost:5002/api/A/${this.id}/refresh`);
    baseUrl.searchParams.append('__dataSource', String(__dataSource));
    const res = await fetchImpl(baseUrl, { method: "GET" });

    return await HttpResult.fromResponse<A>(
      res, 
      MediaType.Json,
      A, false
    );
  }
  static async returnFatalIfParamsNotInstantiated(
    a: A,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/A/returnFatalIfParamsNotInstantiated`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      body: JSON.stringify({
            a
      })
    });

    return await HttpResult.fromResponse<void>(
      res, 
      MediaType.Json,
    );
  }

  static fromJson(data: any, blobs: Uint8Array[]): A {
    const res = Object.assign(new A(), data);
    res["b"] &&= B.fromJson(res.b);


    return res;
  }
}
export class B {
  id: number;

  async testMethod(
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/B/${this.id}/testMethod`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      body: JSON.stringify({
            __dataSource
      })
    });

    return await HttpResult.fromResponse<void>(
      res, 
      MediaType.Json,
    );
  }

  static fromJson(data: any, blobs: Uint8Array[]): B {
    const res = Object.assign(new B(), data);


    return res;
  }
}
export class Course {
  id: number;
  students: Student[];


  static fromJson(data: any, blobs: Uint8Array[]): Course {
    const res = Object.assign(new Course(), data);
    for (let i = 0; i < res.students?.length; i++) {
      res.students[i] = Student.fromJson(res.students[i]);
    }


    return res;
  }
}
export class Dog {
  id: number;
  personId: number;

  async testMethod(
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/Dog/${this.id}/testMethod`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      body: JSON.stringify({
            __dataSource
      })
    });

    return await HttpResult.fromResponse<void>(
      res, 
      MediaType.Json,
    );
  }

  static fromJson(data: any, blobs: Uint8Array[]): Dog {
    const res = Object.assign(new Dog(), data);


    return res;
  }
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
      body: JSON.stringify({
            person
      })
    });

    return await HttpResult.fromResponse<Person>(
      res, 
      MediaType.Json,
      Person, false
    );
  }
  async refresh(
    __dataSource: "withDogs" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Person>> {
    const baseUrl = new URL(`http://localhost:5002/api/Person/${this.id}/refresh`);
    baseUrl.searchParams.append('__dataSource', String(__dataSource));
    const res = await fetchImpl(baseUrl, { method: "GET" });

    return await HttpResult.fromResponse<Person>(
      res, 
      MediaType.Json,
      Person, false
    );
  }
  static async returnFatalIfParamsNotInstantiated(
    person: Person,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/Person/returnFatalIfParamsNotInstantiated`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      body: JSON.stringify({
            person
      })
    });

    return await HttpResult.fromResponse<void>(
      res, 
      MediaType.Json,
    );
  }

  static fromJson(data: any, blobs: Uint8Array[]): Person {
    const res = Object.assign(new Person(), data);
    for (let i = 0; i < res.dogs?.length; i++) {
      res.dogs[i] = Dog.fromJson(res.dogs[i]);
    }


    return res;
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
      body: JSON.stringify({
            student
      })
    });

    return await HttpResult.fromResponse<Student>(
      res, 
      MediaType.Json,
      Student, false
    );
  }
  async refresh(
    __dataSource: "withCoursesStudents" |"withCoursesStudentsCourses" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Student>> {
    const baseUrl = new URL(`http://localhost:5002/api/Student/${this.id}/refresh`);
    baseUrl.searchParams.append('__dataSource', String(__dataSource));
    const res = await fetchImpl(baseUrl, { method: "GET" });

    return await HttpResult.fromResponse<Student>(
      res, 
      MediaType.Json,
      Student, false
    );
  }

  static fromJson(data: any, blobs: Uint8Array[]): Student {
    const res = Object.assign(new Student(), data);
    for (let i = 0; i < res.courses?.length; i++) {
      res.courses[i] = Course.fromJson(res.courses[i]);
    }


    return res;
  }
}
