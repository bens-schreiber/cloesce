// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { B } from "./seed__foreign_keys.cloesce.ts";
import { Course } from "./seed__foreign_keys.cloesce.ts";
import { Person } from "./seed__foreign_keys.cloesce.ts";
import { Student } from "./seed__foreign_keys.cloesce.ts";
import { A } from "./seed__foreign_keys.cloesce.ts";
import { Dog } from "./seed__foreign_keys.cloesce.ts";


const app = new CloesceApp();
const constructorRegistry = {
	B: B,
	Course: Course,
	Person: Person,
	Student: Student,
	A: A,
	Dog: Dog
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    return await app.run(request, env, cidl as any, constructorRegistry);
}

export default { fetch };