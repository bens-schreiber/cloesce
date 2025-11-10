// GENERATED CODE. DO NOT MODIFY.
import { cloesce, CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { PooAcceptYield } from "./seed__poo.cloesce.ts";
import { PooA } from "./seed__poo.cloesce.ts";
import { PooB } from "./seed__poo.cloesce.ts";
import { PooC } from "./seed__poo.cloesce.ts";
const app = new CloesceApp();
const constructorRegistry = {
	PooAcceptYield: PooAcceptYield,
	PooA: PooA,
	PooB: PooB,
	PooC: PooC
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
        console.error(JSON.stringify(e));
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