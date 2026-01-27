// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { Foo } from "./seed__app.cloesce.ts";
import { InjectedThing } from "./seed__app.cloesce.ts";

import { Env } from "./seed__app.cloesce.ts";
import main from "./seed__app.cloesce.ts"
const constructorRegistry: Record<string, new () => any> = {
	Foo: Foo,
	InjectedThing: InjectedThing,
	Env: Env
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await main(request, env, app, ctx);
}

export {cidl, constructorRegistry}
export default { fetch };