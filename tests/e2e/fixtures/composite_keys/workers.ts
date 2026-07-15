import { cloesce, Student, Course, StudentCourse, CfEnv } from "./backend.js";
import { HttpResult } from "cloesce";

const CoursesOrderedDescending = Student.CoursesOrderedDescending.impl({
  async list(env, lastId, lastName, limit) {
    // No raw SQL: fetch the seek-filtered (id > lastId AND name > lastName)
    // ascending page via the generated Default data source, then reorder
    // descending and cap to `limit` in JS.
    const res = await Student.GeneratedSource.Default.list(
      env,
      lastId,
      lastName,
      Number.MAX_SAFE_INTEGER,
    );
    if (!res.ok) {
      return res;
    }
    const students = [...res.data!]
      .sort((a, b) => (a.id !== b.id ? b.id - a.id : b.name.localeCompare(a.name)))
      .slice(0, limit);
    return HttpResult.ok(200, students);
  },
});

const StudentImpl = Student.impl({
  CoursesOrderedDescending,
});

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = cloesce(env);
    app.register(StudentImpl, Course.impl({}), StudentCourse.impl({}));

    return await app.run(request);
  },
};
