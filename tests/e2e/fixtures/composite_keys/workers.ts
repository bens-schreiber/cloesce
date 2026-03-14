// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { Course } from "./seed__composite.cloesce.js";
import { Student } from "./seed__composite.cloesce.js";
import { StudentCourse } from "./seed__composite.cloesce.js";


import { Env } from "./seed__composite.cloesce.js";

const constructorRegistry: Record<string, new () => any> = {
	Course: Course,
	Student: Student,
	StudentCourse: StudentCourse,
	Env: Env
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry, "http://localhost:5104/api");
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };