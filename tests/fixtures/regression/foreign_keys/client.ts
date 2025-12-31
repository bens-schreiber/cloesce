// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8 } from "cloesce/client";


export class A {
  id: number;
  bId: number;
  b: B | undefined;

  static async post(
    a: A,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<A>> {
    
    const baseUrl = new URL(`http://localhost:5002/api/A/post`);
    const payload: any = {};

    payload["a"] = a;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      A,
      false
    );
  }
  async refresh(
    __datasource: "withB" |"withoutB" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<A>> {
    const id = encodeURIComponent(String(this.id));
    const baseUrl = new URL(`http://localhost:5002/api/A/${id}/refresh`);
    

    baseUrl.searchParams.append('__datasource', String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      A,
      false
    );
  }
  static async returnFatalIfParamsNotInstantiated(
    a: A,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    
    const baseUrl = new URL(`http://localhost:5002/api/A/returnFatalIfParamsNotInstantiated`);
    const payload: any = {};

    payload["a"] = a;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): A {
    const res = Object.assign(new A(), data);
    res["b"] &&= .fromJson(res.b);
    return res;
  }
}
export class B {
  id: number;

  async testMethod(
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const id = encodeURIComponent(String(this.id));
    const baseUrl = new URL(`http://localhost:5002/api/B/${id}/testMethod`);
    const payload: any = {};

    baseUrl.searchParams.append('__datasource', String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): B {
    const res = Object.assign(new B(), data);
    return res;
  }
}
export class Course {
  id: number;
  students: Student[];


  static fromJson(data: any): Course {
    const res = Object.assign(new Course(), data);
    for (let i = 0; i < res.students?.length; i++) {
      res.students[i] = .fromJson(res.students[i]);
    }
    return res;
  }
}
export class Dog {
  id: number;
  personId: number;

  async testMethod(
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const id = encodeURIComponent(String(this.id));
    const baseUrl = new URL(`http://localhost:5002/api/Dog/${id}/testMethod`);
    const payload: any = {};

    baseUrl.searchParams.append('__datasource', String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): Dog {
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
    const payload: any = {};

    payload["person"] = person;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      Person,
      false
    );
  }
  async refresh(
    __datasource: "withDogs" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Person>> {
    const id = encodeURIComponent(String(this.id));
    const baseUrl = new URL(`http://localhost:5002/api/Person/${id}/refresh`);
    

    baseUrl.searchParams.append('__datasource', String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      Person,
      false
    );
  }
  static async returnFatalIfParamsNotInstantiated(
    person: Person,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    
    const baseUrl = new URL(`http://localhost:5002/api/Person/returnFatalIfParamsNotInstantiated`);
    const payload: any = {};

    payload["person"] = person;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): Person {
    const res = Object.assign(new Person(), data);
    for (let i = 0; i < res.dogs?.length; i++) {
      res.dogs[i] = .fromJson(res.dogs[i]);
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
    const payload: any = {};

    payload["student"] = student;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      Student,
      false
    );
  }
  async refresh(
    __datasource: "withCoursesStudents" |"withCoursesStudentsCourses" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Student>> {
    const id = encodeURIComponent(String(this.id));
    const baseUrl = new URL(`http://localhost:5002/api/Student/${id}/refresh`);
    

    baseUrl.searchParams.append('__datasource', String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      Student,
      false
    );
  }

  static fromJson(data: any): Student {
    const res = Object.assign(new Student(), data);
    for (let i = 0; i < res.courses?.length; i++) {
      res.courses[i] = .fromJson(res.courses[i]);
    }
    return res;
  }
}

