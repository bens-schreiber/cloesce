// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";


import { FooService } from "./seed__app.cloesce.ts";
import { BarService } from "./seed__app.cloesce.ts";
import main from "./seed__app.cloesce.ts"
const constructorRegistry: Record<string, new () => any> = {
	FooService: FooService,
	BarService: BarService
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await main(request, env, app, ctx);
}

export {cidl, constructorRegistry}
export default { fetch };