import {
  createApp,
  Worker,
  A,
  B,
  Course,
  Person,
  Student,
  Dog,
  CourseStudent,
  type Api,
  type CfEnv,
} from "./backend.js";

const a: Api.A.Of = {
  create(env, model) {
    return env.db.a.save(model);
  },
  withoutB(self) {
    return self;
  },
};

const b: Api.B.Of = {
  testMethod() {},
};

const person: Api.Person.Of = {
  create(env, model) {
    return env.db.person.save(model);
  },
  withoutDogs(self) {
    return self;
  },
};

const dog: Api.Dog.Of = {
  testMethod() {},
};

const student: Api.Student.Of = {
  create(env, model) {
    return env.db.student.withCoursesStudentsCourses.save(model);
  },
  none(self) {
    return self;
  },
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker)
      .register(A, a)
      .register(B, b)
      .register(Person, person)
      .register(Dog, dog)
      .register(Student, student)
      .register(Course, {})
      .register(CourseStudent, {})
      .run(request);
  },
};
