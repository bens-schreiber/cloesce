// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { D1BackedModel } from "./seed__kv.cloesce.ts";
import { PureKVModel } from "./seed__kv.cloesce.ts";



const constructorRegistry: Record<string, new () => any> = {
	D1BackedModel: D1BackedModel,
	PureKVModel: PureKVModel
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };