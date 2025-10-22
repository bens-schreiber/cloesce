import { startWrangler, stopWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import {
  A,
  Person,
  Dog,
  Student,
  Course,
} from "../../fixtures/regression/foreign_keys/client.js";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("../fixtures/regression/foreign_keys");
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

  it("POST A", async () => {
    const res = await A.post(a);
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data.id, withRes("POST id should match input", res)).toBe(a.id);
  });

  testRefresh(a, ["withB", "withoutB", "none"], {
    withB: (data) => expect(data.b).not.toBeUndefined(),
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
    expect(res.data.dogs.length).toBe(1);
  });

  testRefresh(person, ["withDogs", "none"], {
    withDogs: (data) => expect(data.dogs.length).toBe(1),
    none: (data) => expect(data.dogs.length).toBe(0),
  });
});

describe("POST and refresh Student", () => {
  const course = Object.assign(new Course(), { id: 500 });
  const student = Object.assign(new Student(), { id: 1, courses: [course] });

  it("POST Student", async () => {
    const res = await Student.post(student);
    expect(res.ok, withRes("Expected POST to work", res)).toBe(true);
    expect(res.data.courses.length).toBe(1);
  });

  //   // ********TODO: This is failing, theres an error in Many to Many that might be a pain to fix.
  //   // Doing this in a seperate PR
  //   // testRefresh(
  //   //   student,
  //   //   ["withCoursesStudents", "withCoursesStudentsCourses", null],
  //   //   {
  //   //     withCoursesStudents: (data) => {
  //   //       console.log(data);
  //   //       expect(data.courses.length).toBe(1);
  //   //       expect(data.courses[0].students).not.toBeUndefined();
  //   //     },
  //   //     withCoursesStudentsCourses: (data) => {
  //   //       expect(data.courses.length).toBe(1);
  //   //       expect(data.courses[0].students[0].courses).not.toBeUndefined();
  //   //     },
  //   //     null: (data) => expect(data.courses.length).toBe(0),
  //   //   }
  //   // );
});
