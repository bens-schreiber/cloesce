import { cloesce, Student, Course, StudentCourse, Env } from "./backend.js";
import { HttpResult } from "cloesce";

const CoursesOrderedDescending = Student.CoursesOrderedDescending.impl({
  async list(env, lastId, lastName, limit) {
    const stmt = env.db
      .prepare(
        `WITH students AS (${this.selectQueryRaw})
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
    return HttpResult.ok(200, res.value);
  },
});

const StudentImpl = Student.impl({
  CoursesOrderedDescending,
});

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    const app = await cloesce();
    app.register(StudentImpl).register(Course.impl({})).register(StudentCourse.impl({}));

    return await app.run(request, env);
  },
};
