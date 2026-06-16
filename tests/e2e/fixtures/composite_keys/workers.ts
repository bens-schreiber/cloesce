import { cloesce, Student, Course, StudentCourse, CfEnv } from "./backend.js";
import { HttpResult } from "cloesce";

const CoursesOrderedDescending = Student.CoursesOrderedDescending.impl({
  async list(env, lastId, lastName, limit) {
    const stmt = env.db
      .prepare(
        `WITH students AS (${this.selectQuery})
           SELECT * FROM students
           WHERE id > ?1 AND name > ?2
           ORDER BY id DESC, name DESC
           LIMIT ?3`,
      )
      .bind(lastId, lastName, limit);
    const res = await Student.Orm.list(env, { query: stmt, include: this.tree });
    if (res.errors.length > 0) {
      return HttpResult.fail(400, JSON.stringify(res.errors));
    }
    return HttpResult.ok(200, res.value!);
  },
});

const StudentImpl = Student.impl({
  CoursesOrderedDescending,
});

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    const app = cloesce(env);
    app.register(StudentImpl).register(Course.impl({})).register(StudentCourse.impl({}));

    return await app.run(request);
  },
};
