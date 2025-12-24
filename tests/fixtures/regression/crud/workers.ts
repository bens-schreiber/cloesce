// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { CrudHaver } from "./seed__crud.cloesce.ts";
import { Parent } from "./seed__crud.cloesce.ts";
import { Child } from "./seed__crud.cloesce.ts";



const app = new CloesceApp();
const constructorRegistry = {
	CrudHaver: CrudHaver,
	Parent: Parent,
	Child: Child
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    return await app.run(request, env, cidl as any, constructorRegistry);
}

export default { fetch };