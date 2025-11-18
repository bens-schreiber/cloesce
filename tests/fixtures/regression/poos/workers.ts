// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
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
    const envMeta = { envName: "Env", dbName: "db" };
    return await app.run(request, env, cidl as any, constructorRegistry, envMeta);
}

export default { fetch };