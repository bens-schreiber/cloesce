// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { B } from "./seed__foreign_keys.cloesce.ts";
import { Course } from "./seed__foreign_keys.cloesce.ts";
import { Person } from "./seed__foreign_keys.cloesce.ts";
import { Student } from "./seed__foreign_keys.cloesce.ts";
import { A } from "./seed__foreign_keys.cloesce.ts";
import { Dog } from "./seed__foreign_keys.cloesce.ts";



const constructorRegistry: Record<string, new () => any> = {
	B: B,
	Course: Course,
	Person: Person,
	Student: Student,
	A: A,
	Dog: Dog
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };