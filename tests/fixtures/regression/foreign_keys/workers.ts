import { cloesce } from "cloesce/backend";
import cidl from "./cidl.json";
import { A } from "./seed__foreign_keys.cloesce.ts";
import { B } from "./seed__foreign_keys.cloesce.ts";
import { Course } from "./seed__foreign_keys.cloesce.ts";
import { Dog } from "./seed__foreign_keys.cloesce.ts";
import { Person } from "./seed__foreign_keys.cloesce.ts";
import { Student } from "./seed__foreign_keys.cloesce.ts";


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

        try {
            return await cloesce(
                request, 
                cidl, 
                constructorRegistry, 
                instanceRegistry, 
                { envName: "Env", dbName: "db" },  
                "/api"
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
};
