// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { Foo } from "./seed__ds.cloesce.ts";
import { NoDs } from "./seed__ds.cloesce.ts";
import { OneDs } from "./seed__ds.cloesce.ts";
import { Poo } from "./seed__ds.cloesce.ts";


const constructorRegistry: Record<string, new () => any> = {
	Foo: Foo,
	NoDs: NoDs,
	OneDs: OneDs,
	Poo: Poo
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export default { fetch };