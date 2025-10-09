import { cloesce } from "cloesce";
import cidl from "./cidl.json";
import { House } from "./seed__middleware.cloesce.ts";

import { TestMiddleWare } from "./seed__middleware.cloesce.ts";

const constructorRegistry = {
	House: House,
	TestMiddleWare: TestMiddleWare
};

export default {
    async fetch(request: Request, env: any, ctx: any): Promise<Response> {
        const instanceRegistry = new Map([
            ["Env", env]
        ]);

        // Call middleware
        const middlewareInstance = new constructorRegistry.TestMiddleWare();
        const middlewareResult = await middlewareInstance.handle();
        
        // If middleware returns a Response, return it immediately
        if (middlewareResult instanceof Response) {
            return middlewareResult;
        }
        
        // If middleware returns false, return 403 Forbidden
        if (middlewareResult === false) {
            return new Response("Forbidden", { status: 403 });
        }
        
        // If middleware returns true, continue to route handler

        return await cloesce(request, cidl, constructorRegistry, instanceRegistry, { envName: "Env", dbName: "db" },  "/api");
    }
};
