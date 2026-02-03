// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { Weather } from "./seed__bools_dates_ints.cloesce.js";


import { Env } from "./seed__bools_dates_ints.cloesce.js";

const constructorRegistry: Record<string, new () => any> = {
	Weather: Weather,
	Env: Env
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };