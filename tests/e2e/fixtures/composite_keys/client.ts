// GENERATED CODE. DO NOT MODIFY.


export class Course {
  id: number;
  title: string;
  students: StudentCourse[];

  static async GET(
    id: number,
    __datasource: "default" = "default",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Course>> {
    const baseUrl = new URL(
      `http://localhost:5104/api/Course/GET`
    );

    baseUrl.searchParams.append("id", String(id));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "Get",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      Course,
      false
    );
  }
  static async LIST(
    lastSeen_id: number | null,
    limit: number | null,
    offset: number | null,
    __datasource: "default" = "default",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Course[]>> {
    const baseUrl = new URL(
      `http://localhost:5104/api/Course/LIST`
    );

    baseUrl.searchParams.append("lastSeen_id", String(lastSeen_id));
    baseUrl.searchParams.append("limit", String(limit));
    baseUrl.searchParams.append("offset", String(offset));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "Get",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      Course,
      true
    );
  }
  static async SAVE(
    model: DeepPartial<Course>,
    __datasource: "default" = "default",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Course>> {
    const baseUrl = new URL(
      `http://localhost:5104/api/Course/SAVE`
    );
    const payload: any = {};

    payload["model"] = model;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "Post",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      Course,
      false
    );
  }

  static fromJson(data: any): Course {
    const res = Object.assign(new Course(), data);
    for (let i = 0; i < res.students?.length; i++) {
      res.students[i] = StudentCourse.fromJson(res.students[i]);
    }
    return res;
  }
}
export class Student {
  id: number;
  name: string;
  favoriteColor: string;
  courses: StudentCourse[];

  static async GET(
    id: number,
    name: string,
    __datasource: "coursesOrderedDesc" | "default" = "default",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Student>> {
    const baseUrl = new URL(
      `http://localhost:5104/api/Student/GET`
    );

    baseUrl.searchParams.append("id", String(id));
    baseUrl.searchParams.append("name", String(name));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "Get",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      Student,
      false
    );
  }
  static async LIST(
    lastSeen_id: number | null,
    lastSeen_name: string | null,
    limit: number | null,
    offset: number | null,
    __datasource: "coursesOrderedDesc" | "default" = "default",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Student[]>> {
    const baseUrl = new URL(
      `http://localhost:5104/api/Student/LIST`
    );

    baseUrl.searchParams.append("lastSeen_id", String(lastSeen_id));
    baseUrl.searchParams.append("lastSeen_name", String(lastSeen_name));
    baseUrl.searchParams.append("limit", String(limit));
    baseUrl.searchParams.append("offset", String(offset));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "Get",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      Student,
      true
    );
  }
  static async SAVE(
    model: DeepPartial<Student>,
    __datasource: "coursesOrderedDesc" | "default" = "default",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Student>> {
    const baseUrl = new URL(
      `http://localhost:5104/api/Student/SAVE`
    );
    const payload: any = {};

    payload["model"] = model;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "Post",
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

  static fromJson(data: any): Student {
    const res = Object.assign(new Student(), data);
    for (let i = 0; i < res.courses?.length; i++) {
      res.courses[i] = StudentCourse.fromJson(res.courses[i]);
    }
    return res;
  }
}
export class StudentCourse {
  studentId: number;
  studentName: string;
  courseId: number;
  student: Student | undefined;
  course: Course | undefined;

  static async GET(
    studentId: number,
    studentName: string,
    courseId: number,
    __datasource: "default" | "withStudentCourse" = "default",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<StudentCourse>> {
    const baseUrl = new URL(
      `http://localhost:5104/api/StudentCourse/GET`
    );

    baseUrl.searchParams.append("studentId", String(studentId));
    baseUrl.searchParams.append("studentName", String(studentName));
    baseUrl.searchParams.append("courseId", String(courseId));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "Get",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      StudentCourse,
      false
    );
  }
  static async LIST(
    lastSeen_studentId: number | null,
    lastSeen_studentName: string | null,
    lastSeen_courseId: number | null,
    limit: number | null,
    offset: number | null,
    __datasource: "default" | "withStudentCourse" = "default",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<StudentCourse[]>> {
    const baseUrl = new URL(
      `http://localhost:5104/api/StudentCourse/LIST`
    );

    baseUrl.searchParams.append("lastSeen_studentId", String(lastSeen_studentId));
    baseUrl.searchParams.append("lastSeen_studentName", String(lastSeen_studentName));
    baseUrl.searchParams.append("lastSeen_courseId", String(lastSeen_courseId));
    baseUrl.searchParams.append("limit", String(limit));
    baseUrl.searchParams.append("offset", String(offset));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "Get",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      StudentCourse,
      true
    );
  }
  static async SAVE(
    model: DeepPartial<StudentCourse>,
    __datasource: "default" | "withStudentCourse" = "default",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<StudentCourse>> {
    const baseUrl = new URL(
      `http://localhost:5104/api/StudentCourse/SAVE`
    );
    const payload: any = {};

    payload["model"] = model;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "Post",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      StudentCourse,
      false
    );
  }

  static fromJson(data: any): StudentCourse {
    const res = Object.assign(new StudentCourse(), data);
    res["student"] &&= Student.fromJson(res.student);
    res["course"] &&= Course.fromJson(res.course);
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

export interface Paginated<T> {
  results: T[];
  cursor: string | null;
  complete: boolean;
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
          return response;
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