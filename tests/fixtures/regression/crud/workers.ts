import { cloesce } from "cloesce/backend";
import cidl from "./cidl.json";
import { Child } from "./seed__crud.cloesce.ts";
import { CrudHaver } from "./seed__crud.cloesce.ts";
import { Parent } from "./seed__crud.cloesce.ts";


const constructorRegistry = {
	Child: Child,
	CrudHaver: CrudHaver,
	Parent: Parent
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
