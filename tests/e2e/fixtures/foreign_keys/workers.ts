import * as clo from "./backend.js";

const A = clo.A.impl({
  async create(e, a) {
    return (await this.Orm.save(e, a)).value!;
  },
  withoutB(self) {
    return self;
  },
});

const B = clo.B.impl({
  testMethod() {},
});

const Person = clo.Person.impl({
  async create(e, person) {
    return (await this.Default.save(e, person)) as any;
  },

  withoutDogs(self) {
    return self;
  },
});

const Dog = clo.Dog.impl({
  testMethod() {},
});

const Student = clo.Student.impl({
  async create(e, student) {
    return (await this.WithCoursesStudentsCourses.save(e, student)) as any;
  },
  none(self) {
    return self;
  },
});

const Course = clo.Course.impl({});

export default {
  async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
    const app = clo.cloesce(env);
    app.register(A).register(B).register(Person).register(Dog).register(Student).register(Course);

    return await app.run(request);
  },
};
