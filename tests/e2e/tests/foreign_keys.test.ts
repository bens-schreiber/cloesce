import { startWrangler, stopWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import {
  A,
  Person,
  Dog,
  Student,
  Course,
  B,
} from "../fixtures/foreign_keys/client";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("./fixtures/foreign_keys");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

async function testRefresh<T>(
  obj: T & { refresh: (dataSource: any) => Promise<any> },
  dataSources: any[],
  assertions: Record<string, (res: any) => void>,
) {
  for (const ds of dataSources) {
    it(`refresh ${ds}`, async () => {
      const res = await obj.refresh(ds);
      expect(res.ok, withRes("Expected refresh to work", res)).toBe(true);
      assertions[ds]?.(res.data);
    });
  }
}

describe("POST and refresh A", () => {
  const a = Object.assign(new A(), {
    id: 1,
    bId: 10,
    b: {
      id: 10,
    },
  });

  let b: B;
  it("POST A", async () => {
    const res = await A.post(a);
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data!.id, withRes("POST id should match input", res)).toBe(a.id);
    b = res.data!.b!;
  });

  it("Object to be instantiated on backend", async () => {
    const res = await A.returnFatalIfParamsNotInstantiated(a);
    expect(
      res.ok,
      withRes("Objects should be instantiated on the backend", res),
    ).toBe(true);
  });

  it("Inner object is instantiated", async () => {
    expect(b.testMethod).toBeDefined();
  });

  testRefresh(a, ["withB", "withoutB", "none"], {
    withB: (data) => expect(data.b).toBeDefined(),
    withoutB: (data) => expect(data.b).toBeUndefined(),
    none: () => {},
  });
});

describe("POST and refresh Person", () => {
  const person = Object.assign(new Person(), {
    id: 1,
    dogs: [Object.assign(new Dog(), { id: 101, personId: 1 })],
  });

  it("POST Person", async () => {
    const res = await Person.post(person);
    expect(res.ok).toBe(true);
    expect(res.data!.dogs.length).toBe(1);
  });

  testRefresh(person, ["withDogs", "none"], {
    withDogs: (data) => expect(data.dogs.length).toBe(1),
    none: (data) => expect(data.dogs.length).toBe(0),
  });
});

describe("POST and refresh Student", () => {
  const course = Object.assign(new Course(), { id: 500, students: [] });
  const student = Object.assign(new Student(), { id: 1, courses: [course] });

  it("POST Student", async () => {
    const res = await Student.post(student);
    expect(res.ok, withRes("Expected POST to work", res)).toBe(true);
    expect(res.data!.courses.length).toBe(1);
  });

  testRefresh(
    student,
    ["withCoursesStudents", "withCoursesStudentsCourses", "none"],
    {
      withCoursesStudents: (data) => {
        expect(data.courses.length).toBe(1);
        expect(data.courses[0].students).not.toBeUndefined();
      },
      withCoursesStudentsCourses: (data) => {
        expect(data.courses.length).toBe(1);
        expect(data.courses[0].students[0].courses).not.toBeUndefined();
      },
      none: (data) => expect(data.courses.length).toBe(0),
    },
  );
});
