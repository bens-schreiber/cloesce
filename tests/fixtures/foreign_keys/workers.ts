
import { cloesce } from "cloesce";
import cidl from "./cidl.json";
import { A } from "./foreign_keys.cloesce";
import { B } from "./foreign_keys.cloesce";
import { Course } from "./foreign_keys.cloesce";
import { Dog } from "./foreign_keys.cloesce";
import { Person } from "./foreign_keys.cloesce";
import { Student } from "./foreign_keys.cloesce";

const constructorRegistry = {
	A: A,
	B: B,
	Course: Course,
	Dog: Dog,
	Person: Person,
	Student: Student
};

export default {
    async fetch(request: Request, env: any, ctx: any): Promise<Response> {
        const instanceRegistry = new Map([
            ["Env", env]
        ]);

        return await cloesce(request, cidl, constructorRegistry, instanceRegistry, { envName: "Env", dbName: "D1_DB" },  "/api");
    }
};
