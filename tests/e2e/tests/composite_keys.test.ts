import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, expectHttpResult } from "../src/setup";
import { Student, Course, StudentCourse } from "../fixtures/composite_keys/client";
import config from "../fixtures/composite_keys/cloesce.jsonc" with { type: "jsonc" };

let stopWrangler: () => Promise<void>;
beforeAll(async () => {
  // NOTE: e2e is called from proj root
  stopWrangler = await startWrangler("./fixtures/composite_keys", config.workers_url!);
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("Student Enrollment", () => {
  let student1: Student;
  let student2: Student;
  let student3: Student;
  let course1: Course;
  let course2: Course;
  let enrollment1: StudentCourse;
  let enrollment2: StudentCourse;
  let enrollment3: StudentCourse;

  // Student CRUD tests
  it("POST - Create students with composite key (id, name)", async () => {
    const res1 = await Student.$save({
      id: 1,
      name: "Alice",
      favoriteColor: "Red",
    });
    const res2 = await Student.$save({
      id: 2,
      name: "Bob",
      favoriteColor: "Green",
    });
    const res3 = await Student.$save({
      id: 3,
      name: "Charlie",
      favoriteColor: "Blue",
    });

    expectHttpResult(res1, "POST student 1 should be OK");
    expectHttpResult(res2, "POST student 2 should be OK");
    expectHttpResult(res3, "POST student 3 should be OK");

    student1 = res1.data!;
    student2 = res2.data!;
    student3 = res3.data!;

    expect(student1).toEqual({
      id: 1,
      name: "Alice",
      favoriteColor: "Red",
      studentCourses: [],
    });
    expect(student2).toEqual({
      id: 2,
      name: "Bob",
      favoriteColor: "Green",
      studentCourses: [],
    });
    expect(student3).toEqual({
      id: 3,
      name: "Charlie",
      favoriteColor: "Blue",
      studentCourses: [],
    });
  });

  it("$get - Retrieve a student by composite key (id, name)", async () => {
    const res = await Student.$get(1, "Alice");
    expectHttpResult(res, "$get should be OK");
    expect(res.data).toEqual(student1);
  });

  it("POST - Update a student", async () => {
    student1.favoriteColor = "Purple";
    const res = await Student.$save(student1);
    expectHttpResult(res, "POST update should be OK");
    expect(res.data?.favoriteColor).toBe("Purple");
    student1 = res.data!;
  });

  it("$list - Retrieve all students", async () => {
    const res = await Student.$list(0, "", 100);
    expectHttpResult(res, "$list should be OK");
    expect(res.data!.length).toBe(3);
    expect(res.data!.map((s) => s.id)).toContain(1);
    expect(res.data!.map((s) => s.id)).toContain(2);
    expect(res.data!.map((s) => s.id)).toContain(3);
  });

  it("$list - Paginate students with limit", async () => {
    const res = await Student.$list(0, "", 2);
    expectHttpResult(res, "$list with limit should be OK");
    expect(res.data!.length).toBe(2);
  });

  // Create courses for StudentCourse tests
  it("POST - Create courses", async () => {
    const courseRes1 = await Course.$save({
      id: 1,
      title: "Mathematics",
    });
    const courseRes2 = await Course.$save({
      id: 2,
      title: "Computer Science",
    });

    expectHttpResult(courseRes1, "POST course 1 should be OK");
    expectHttpResult(courseRes2, "POST course 2 should be OK");

    course1 = courseRes1.data!;
    course2 = courseRes2.data!;
  });

  // StudentCourse CRUD tests
  it("POST - Create StudentCourse with composite FK/PK", async () => {
    // Alice enrolls in Mathematics and Computer Science
    const res1 = await StudentCourse.$save({
      studentId: student1.id,
      studentName: student1.name,
      courseId: course1.id,
    });
    const res2 = await StudentCourse.$save({
      studentId: student1.id,
      studentName: student1.name,
      courseId: course2.id,
    });
    // Bob enrolls in Computer Science
    const res3 = await StudentCourse.$save({
      studentId: student2.id,
      studentName: student2.name,
      courseId: course2.id,
    });

    expectHttpResult(res1, "POST enrollment 1 should be OK");
    expectHttpResult(res2, "POST enrollment 2 should be OK");
    expectHttpResult(res3, "POST enrollment 3 should be OK");

    enrollment1 = res1.data!;
    enrollment2 = res2.data!;
    enrollment3 = res3.data!;

    expect(enrollment1).toEqual({
      studentId: student1.id,
      studentName: student1.name,
      courseId: course1.id,
    });
    expect(enrollment2).toEqual({
      studentId: student1.id,
      studentName: student1.name,
      courseId: course2.id,
    });
    expect(enrollment3).toEqual({
      studentId: student2.id,
      studentName: student2.name,
      courseId: course2.id,
    });
  });

  it("$get - Retrieve StudentCourse by composite key (studentId, studentName, courseId)", async () => {
    const res = await StudentCourse.$get(student1.id, student1.name, course1.id);
    expectHttpResult(res, "$get should be OK");
    expect(res.data).toEqual(enrollment1);
  });

  it("$get - Retrieve another StudentCourse by composite key", async () => {
    const res = await StudentCourse.$get(student2.id, student2.name, course2.id);
    expectHttpResult(res, "$get should be OK");
    expect(res.data).toEqual(enrollment3);
  });

  it("$list - Retrieve all StudentCourse entries", async () => {
    const res = await StudentCourse.$list(0, "", 0, 100);
    expectHttpResult(res, "$list should be OK");
    expect(res.data!.length).toBe(3);
  });

  it("$list - Paginate StudentCourse with limit", async () => {
    const res = await StudentCourse.$list(0, "", 0, 2);
    expectHttpResult(res, "$list with limit should be OK");
    expect(res.data!.length).toBe(2);
  });

  it("coursesOrderedDesc data source", async () => {
    // $list - Use coursesOrderedDesc data source with default params
    const $listRes = await Student.$list_CoursesOrderedDescending(0, "", 100);
    expectHttpResult($listRes, "$list with coursesOrderedDesc should be OK");
    expect($listRes.data).toBeDefined();
    expect(Array.isArray($listRes.data)).toBe(true);

    // $list - Use coursesOrderedDesc with limit parameter
    const limitRes = await Student.$list_CoursesOrderedDescending(0, "", 3);
    expectHttpResult(limitRes, "$list with coursesOrderedDesc and limit should be OK");
    expect(limitRes.data!.length).toBeLessThanOrEqual(3);

    // $list - coursesOrderedDesc should order by studentId DESC, studentName DESC
    const orderRes = await Student.$list_CoursesOrderedDescending(0, "", 100);
    expectHttpResult(orderRes, "$list with coursesOrderedDesc should be OK");

    if (orderRes.data!.length > 1) {
      // Verify descending order by studentId
      for (let i = 0; i < orderRes.data!.length - 1; i++) {
        const current = orderRes.data![i];
        const next = orderRes.data![i + 1];

        // studentId should be descending
        if (current.id === next.id) {
          // If ids are equal, name should be descending
          expect(current.name.localeCompare(next.name)).toBeGreaterThanOrEqual(0);
        } else {
          expect(current.id).toBeGreaterThanOrEqual(next.id);
        }
      }
    }

    // $list - coursesOrderedDesc should include courses in results
    const coursesRes = await Student.$list_CoursesOrderedDescending(0, "", 100);
    expectHttpResult(coursesRes, "$list with coursesOrderedDesc should be OK");

    const studentWithCourses = coursesRes.data!.find(
      (s) => s.studentCourses && s.studentCourses.length > 0,
    );

    if (studentWithCourses) {
      expect(studentWithCourses.studentCourses).toBeDefined();
      expect(Array.isArray(studentWithCourses.studentCourses)).toBe(true);
      expect(studentWithCourses.studentCourses.length).toBeGreaterThan(0);
    }
  });

  it("POST - Create a StudentCourse, Student, and Course in one request", async () => {
    const res = await StudentCourse.$save_WithStudentCourse({
      studentId: 10,
      studentName: "Jack",
      courseId: 10,
      student: {
        id: 10,
        name: "Jack",
        favoriteColor: "Yellow",
      },
      course: {
        id: 10,
        title: "History",
      },
    });

    expectHttpResult(res, "POST StudentCourse with nested Student and Course should be OK");

    expect(res.data).toEqual({
      studentId: 10,
      studentName: "Jack",
      courseId: 10,
      student: {
        id: 10,
        name: "Jack",
        favoriteColor: "Yellow",
        studentCourses: [
          {
            studentId: 10,
            studentName: "Jack",
            courseId: 10,
            student: undefined,
            course: undefined,
          },
        ],
      },
      course: {
        id: 10,
        title: "History",
        studentCourses: [
          {
            studentId: 10,
            studentName: "Jack",
            courseId: 10,
            student: undefined,
            course: undefined,
          },
        ],
      },
    });
  });
});
