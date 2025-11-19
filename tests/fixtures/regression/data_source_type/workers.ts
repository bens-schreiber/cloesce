// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { Foo } from "./seed__ds.cloesce.ts";
import { NoDs } from "./seed__ds.cloesce.ts";
import { OneDs } from "./seed__ds.cloesce.ts";
import { Poo } from "./seed__ds.cloesce.ts";

const app = new CloesceApp();
const constructorRegistry = {
	Foo: Foo,
	NoDs: NoDs,
	OneDs: OneDs,
	Poo: Poo
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const envMeta = { envName: "Env", dbName: "db" };
    return await app.run(request, env, cidl as any, constructorRegistry, envMeta);
}

export default { fetch };