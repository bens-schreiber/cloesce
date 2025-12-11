// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { Model } from "./seed__app.cloesce.ts";
import { InjectedThing } from "./seed__app.cloesce.ts";

import app from "./seed__app.cloesce.ts"
const constructorRegistry = {
	Model: Model,
	InjectedThing: InjectedThing
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    return await app.run(request, env, cidl as any, constructorRegistry);
}

export default { fetch };