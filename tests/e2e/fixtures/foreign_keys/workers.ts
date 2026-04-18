import { HttpResult } from "cloesce";
import * as Cloesce from "./backend.js";

const A = Cloesce.A.impl({
    async create(e, a) {
        return (await this.Orm.save(e, a))!;
    },
    withoutB(self) {
        return self;
    },
});

const B = Cloesce.B.impl({
    testMethod(self) { }
});

const Person = Cloesce.Person.impl({
    async create(e, person) {
        return (await this.Orm.save(e, person))!;
    },

    withoutDogs(self) {
        return self;
    },
});

const Dog = Cloesce.Dog.impl({
    testMethod(self) {
    }
});

const Student = Cloesce.Student.impl({
    async create(e, student) {
        return (await this.Orm.save(e, student, this.WithCoursesStudentsCourses.include))!;
    },
    none(self) {
        return self;
    },
});

export default {
    async fetch(request: Request, env: Cloesce.Env): Promise<Response> {
        const app = await Cloesce.cloesce();
        app.register(A)
            .register(B)
            .register(Person)
            .register(Dog)
            .register(Student);

        return await app.run(request, env);
    }
}