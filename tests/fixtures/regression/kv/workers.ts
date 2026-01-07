// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { D1BackedModel } from "./seed__kv.cloesce.ts";
import { PureKVModel } from "./seed__kv.cloesce.ts";


const app = new CloesceApp();
const constructorRegistry = {
	D1BackedModel: D1BackedModel,
	PureKVModel: PureKVModel
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    return await app.run(request, env, cidl as any, constructorRegistry);
}

export default { fetch };