import { cloesce } from "cloesce";
import cidl from "./cidl.json";
import { NullabilityChecks } from "./seed__nullability.cloesce.ts";


const constructorRegistry = {
	NullabilityChecks: NullabilityChecks
};

export default {
    async fetch(request: Request, env: any, ctx: any): Promise<Response> {
        const instanceRegistry = new Map([
            ["Env", env]
        ]);

        return await cloesce(request, cidl, constructorRegistry, instanceRegistry, { envName: "Env", dbName: "db" },  "/api");
    }
};
