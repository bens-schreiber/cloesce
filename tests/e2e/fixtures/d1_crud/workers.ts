// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { CrudHaver } from "./seed__d1_crud.cloesce.js";
import { Parent } from "./seed__d1_crud.cloesce.js";
import { Child } from "./seed__d1_crud.cloesce.js";


import { Env } from "./seed__d1_crud.cloesce.js";

const constructorRegistry: Record<string, new () => any> = {
	CrudHaver: CrudHaver,
	Parent: Parent,
	Child: Child,
	Env: Env
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };