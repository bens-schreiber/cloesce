import { startWrangler, withRes } from "../src/setup.js";
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import {
  A,
  Person,
  Dog,
  Student,
  Course,
  B,
} from "../fixtures/foreign_keys/client";
import config from "../fixtures/foreign_keys/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler(
    "./fixtures/foreign_keys",
    config.workers_url!,
  );
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

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
    const res = await A.create(a);
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data!.id, withRes("POST id should match input", res)).toBe(a.id);
    b = res.data!.b!;
  });

  it("Inner object is instantiated", async () => {
    expect(b.testMethod).toBeDefined();
  });

  it("withoutB returns A without B", async () => {
    const res = await a.withoutB();
    expect(res.data?.id).toBe(a.id);
    expect(res.data?.b).toBeUndefined();
  });
});

describe("POST and refresh Person", () => {
  const person = Object.assign(new Person(), {
    id: 1,
    dogs: [Object.assign(new Dog(), { id: 101, personId: 1 })],
  });

  it("POST Person", async () => {
    const res = await Person.create(person);
    expect(res.ok).toBe(true);
    expect(res.data!.dogs.length).toBe(1);
  });

  it("withoutDogs returns Person without Dogs", async () => {
    const res = await person.withoutDogs();
    expect(res.data?.id).toBe(person.id);
    expect(res.data?.dogs.length).toBe(0);
  });
});

describe("POST and refresh Student", () => {
  const course = Object.assign(new Course(), { id: 500, students: [] });
  const student = Object.assign(new Student(), { id: 1, courses: [course] });

  it("POST Student", async () => {
    const res = await Student.create(student);
    expect(res.ok, withRes("Expected POST to work", res)).toBe(true);
    expect(res.data!.courses.length).toBe(1);

    // should have returned a student with courses, which should have students, which should have courses
    expect(res.data!.courses[0].students.length).toBe(1);
    expect(res.data!.courses[0].students[0].courses.length).toBe(1);
  });

  it("none returns Student without Courses", async () => {
    const res = await student.none();
    expect(res.data?.id).toBe(student.id);
    expect(res.data?.courses.length).toBe(0);
  });
});
