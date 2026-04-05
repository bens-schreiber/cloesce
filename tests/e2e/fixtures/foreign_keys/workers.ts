import { HttpResult } from "cloesce";
import * as Cloesce from "./backend.js";

class A extends Cloesce.A.Api {
    async create(e: Cloesce.Env, a: Cloesce.A.Self): Promise<Cloesce.A.Self> {
        return (await Cloesce.A.save(e, a))!;
    }
    withoutB(self: Cloesce.A.Self): Cloesce.A.Self {
        return self;
    }
}

class B extends Cloesce.B.Api {
    testMethod(self: Cloesce.B.Self): void { }
}

class Person extends Cloesce.Person.Api {
    async create(e: Cloesce.Env, person: Cloesce.Person.Self): Promise<Cloesce.Person.Self> {
        return (await Cloesce.Person.save(e, person))!;
    }

    withoutDogs(self: Cloesce.Person.Self): Cloesce.Person.Self {
        return self;
    }
}

class Dog extends Cloesce.Dog.Api {
    testMethod(self: Cloesce.Dog.Self): void {
    }
}

class Student extends Cloesce.Student.Api {
    async create(e: Cloesce.Env, student: Cloesce.Student.Self): Promise<Cloesce.Student.Self> {
        return (await Cloesce.Student.save(e, student))!;
    }
    none(self: Cloesce.Student.Self): Cloesce.Student.Self {
        return self;
    }
}

export default {
    async fetch(request: Request, env: Cloesce.Env): Promise<Response> {
        const app = await Cloesce.cloesce();
        app.register(new A())
            .register(new B())
            .register(new Person())
            .register(new Dog())
            .register(new Student());

        return await app.run(request, env);
    }
}