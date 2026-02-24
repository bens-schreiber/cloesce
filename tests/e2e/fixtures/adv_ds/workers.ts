// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { Hamburger } from "./seed__adv_ds.cloesce.js";
import { Topping } from "./seed__adv_ds.cloesce.js";


import { Env } from "./seed__adv_ds.cloesce.js";

const constructorRegistry: Record<string, new () => any> = {
	Hamburger: Hamburger,
	Topping: Topping,
	Env: Env
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };