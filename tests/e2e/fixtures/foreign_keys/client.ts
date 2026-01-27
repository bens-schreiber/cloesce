// GENERATED CODE. DO NOT MODIFY.


export class A {
  id: number;
  bId: number;
  b: B | undefined;

  static async post(
    a: A,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<A>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/A/post`
    );
    const payload: any = {};

    payload["a"] = a;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
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
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/A/${id}/refresh`
    );

    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
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
    const baseUrl = new URL(
      `http://localhost:5002/api/A/returnFatalIfParamsNotInstantiated`
    );
    const payload: any = {};

    payload["a"] = a;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
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
    res["b"] &&= B.fromJson(res.b);
    return res;
  }
}
export class B {
  id: number;

  async testMethod(
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/B/${id}/testMethod`
    );
    const payload: any = {};

    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
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
      res.students[i] = Student.fromJson(res.students[i]);
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
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/Dog/${id}/testMethod`
    );
    const payload: any = {};

    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
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
    const baseUrl = new URL(
      `http://localhost:5002/api/Person/post`
    );
    const payload: any = {};

    payload["person"] = person;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
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
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/Person/${id}/refresh`
    );

    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
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
    const baseUrl = new URL(
      `http://localhost:5002/api/Person/returnFatalIfParamsNotInstantiated`
    );
    const payload: any = {};

    payload["person"] = person;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
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
    const baseUrl = new URL(
      `http://localhost:5002/api/Student/post`
    );
    const payload: any = {};

    payload["student"] = student;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
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
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/Student/${id}/refresh`
    );

    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
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
      res.courses[i] = Course.fromJson(res.courses[i]);
    }
    return res;
  }
}

type DeepPartialInner<T> = T extends (infer U)[]
  ? DeepPartialInner<U>[]
  : T extends object
  ? { [K in keyof T]?: DeepPartialInner<T[K]> }
  : T | (null extends T ? null : never);
export type DeepPartial<T> = DeepPartialInner<T> & { __brand?: "Partial" };

export class KValue<V> {
  key!: string;
  raw: unknown | null;
  metadata: unknown | null;
  get value(): V | null {
    return this.raw as V | null;
  }
}

export enum MediaType {
  Json = "Json",
  Octet = "Octet",
}

declare const Buffer: any;
export function b64ToU8(b64: string): Uint8Array {
  if (typeof Buffer !== "undefined") {
    const buffer = Buffer.from(b64, "base64");
    return new Uint8Array(buffer);
  }
  const s = atob(b64);
  const u8 = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) {
    u8[i] = s.charCodeAt(i);
  }
  return u8;
}

export function u8ToB64(u8: Uint8Array): string {
  if (typeof Buffer !== "undefined") {
    return Buffer.from(u8).toString("base64");
  }
  let s = "";
  for (let i = 0; i < u8.length; i++) {
    s += String.fromCharCode(u8[i]);
  }
  return btoa(s);
}

export class R2Object {
  key!: string;
  version!: string;
  size!: number;
  etag!: string;
  httpEtag!: string;
  uploaded!: Date;
  customMetadata?: Record<string, string>;
}

function requestBody(
  mediaType: MediaType,
  data: any | string | undefined,
): BodyInit | undefined {
  switch (mediaType) {
    case MediaType.Json: {
      return JSON.stringify(data ?? {}, (_, v) => {
        if (v instanceof Uint8Array) {
          return u8ToB64(v);
        }
        return v;
      });
    }
    case MediaType.Octet: {
      return Object.values(data)[0] as BodyInit;
    }
  }
}

export class HttpResult<T = unknown> {
  public constructor(
    public ok: boolean,
    public status: number,
    public headers: Headers,
    public data?: T,
    public message?: string,
    public mediaType?: MediaType,
  ) { }

  static async fromResponse(
    response: Response,
    mediaType: MediaType,
    ctor?: any,
    array: boolean = false,
  ): Promise<HttpResult<any>> {
    if (response.status >= 400) {
      return new HttpResult(
        false,
        response.status,
        response.headers,
        undefined,
        await response.text(),
      );
    }

    function instantiate(json: any, ctor?: any) {
      switch (ctor) {
        case Date: {
          return new Date(json);
        }
        case Uint8Array: {
          return b64ToU8(json);
        }
        case undefined: {
          return json;
        }
        default: {
          return ctor.fromJson(json);
        }
      }
    }

    async function data() {
      switch (mediaType) {
        case MediaType.Json: {
          const data = await response.json();

          if (array && Array.isArray(data)) {
            for (let i = 0; i < data.length; i++) {
              data[i] = instantiate(data[i], ctor);
            }
            return data;
          }
          return instantiate(data, ctor);
        }
        case MediaType.Octet: {
          return response.body;
        }
      }
    }
    return new HttpResult(
      true,
      response.status,
      response.headers,
      await data(),
    );
  }
}