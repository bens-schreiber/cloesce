// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { CrudHaver } from "./seed__crud.cloesce.ts";
import { Parent } from "./seed__crud.cloesce.ts";
import { Child } from "./seed__crud.cloesce.ts";


import { Env } from "./seed__crud.cloesce.ts";

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