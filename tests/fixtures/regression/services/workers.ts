// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";


import { FooService } from "./seed__app.cloesce.ts";
import { BarService } from "./seed__app.cloesce.ts";
import app from "./seed__app.cloesce.ts"
const constructorRegistry = {
	FooService: FooService,
	BarService: BarService
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const envMeta = { envName: "Env", dbName: "db" };
    return await app.run(request, env, cidl as any, constructorRegistry, envMeta);
}

export default { fetch };