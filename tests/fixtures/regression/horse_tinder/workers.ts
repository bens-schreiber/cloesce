import { cloesce } from "cloesce";
import cidl from "./cidl.json";
import { Horse } from "./seed__horse_tinder.cloesce.ts";
import { Like } from "./seed__horse_tinder.cloesce.ts";


const constructorRegistry = {
	Horse: Horse,
	Like: Like
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
