import { cloesce, CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { A } from "./seed__foreign_keys.cloesce.ts";
import { B } from "./seed__foreign_keys.cloesce.ts";
import { Course } from "./seed__foreign_keys.cloesce.ts";
import { Dog } from "./seed__foreign_keys.cloesce.ts";
import { Person } from "./seed__foreign_keys.cloesce.ts";
import { Student } from "./seed__foreign_keys.cloesce.ts";

const app = new CloesceApp()
const constructorRegistry = {
	A: A,
	B: B,
	Course: Course,
	Dog: Dog,
	Person: Person,
	Student: Student
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    try {
        const envMeta = { envName: "Env", dbName: "db" };
        const apiRoute = "/api";
        return await cloesce(
            request, 
            env,
            cidl, 
            app,
            constructorRegistry, 
            envMeta,  
            apiRoute
        );
    } catch(e: any) {
        return new Response(JSON.stringify({
            ok: false,
            status: 500,
            message: e.toString()
        }), {
            status: 500,
            headers: { "Content-Type": "application/json" },
            });
    }
}

export default {fetch};