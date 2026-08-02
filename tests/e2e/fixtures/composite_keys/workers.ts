import { createApp, Student, Course, StudentCourse, type Api, type CfEnv } from "./backend.js";

const CoursesOrderedDescending: Api.Student.CoursesOrderedDescending = {
  async list(env, lastId, lastName, limit) {
    const query = env.db
      .prepare(
        "SELECT * FROM Student WHERE (id, name) > (?, ?) ORDER BY id DESC, name DESC LIMIT ?",
      )
      .bind(lastId, lastName, limit);

    const students = (await query.all<Student>()).results;
    return env.db.Student.CoursesOrderedDescending.hydrateAll(students);
  },
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp()
      .worker(env)
      .register(Student, { CoursesOrderedDescending })
      .register(Course, {})
      .register(StudentCourse, {})
      .run(request);
  },
};
