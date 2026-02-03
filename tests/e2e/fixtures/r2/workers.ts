// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { D1BackedModel } from "./seed__r2.cloesce.js";
import { PureR2Model } from "./seed__r2.cloesce.js";


import { Env } from "./seed__r2.cloesce.js";

const constructorRegistry: Record<string, new () => any> = {
	D1BackedModel: D1BackedModel,
	PureR2Model: PureR2Model,
	Env: Env
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };