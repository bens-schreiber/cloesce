// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { PooAcceptYield } from "./seed__poo.cloesce.ts";
import { PooA } from "./seed__poo.cloesce.ts";
import { PooB } from "./seed__poo.cloesce.ts";
import { PooC } from "./seed__poo.cloesce.ts";

import { Env } from "./seed__poo.cloesce.ts";

const constructorRegistry: Record<string, new () => any> = {
	PooAcceptYield: PooAcceptYield,
	PooA: PooA,
	PooB: PooB,
	PooC: PooC,
	Env: Env
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };