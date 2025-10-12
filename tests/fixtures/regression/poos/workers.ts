import { cloesce } from "cloesce";
import cidl from "./cidl.json";
import { PooAcceptYield } from "./seed__poo.cloesce.ts";
import { PooA } from "./seed__poo.cloesce.ts";
import { PooB } from "./seed__poo.cloesce.ts";
import { PooC } from "./seed__poo.cloesce.ts";

const constructorRegistry = {
	PooAcceptYield: PooAcceptYield,
	PooA: PooA,
	PooB: PooB,
	PooC: PooC
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
