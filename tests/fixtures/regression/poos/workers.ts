
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

        return await cloesce(request, cidl, constructorRegistry, instanceRegistry, { envName: "Env", dbName: "db" },  "/api");
    }
};
