// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { Foo } from "./seed__ds.cloesce.js";
import { NoDs } from "./seed__ds.cloesce.js";
import { OneDs } from "./seed__ds.cloesce.js";
import { Poo } from "./seed__ds.cloesce.js";

import { Env } from "./seed__ds.cloesce.js";

const constructorRegistry: Record<string, new () => any> = {
	Foo: Foo,
	NoDs: NoDs,
	OneDs: OneDs,
	Poo: Poo,
	Env: Env
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };