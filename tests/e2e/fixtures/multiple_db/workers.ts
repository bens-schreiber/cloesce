// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { DB1Model } from "./seed__multi_db.cloesce.js";
import { DB2Model } from "./seed__multi_db.cloesce.js";


import { Env } from "./seed__multi_db.cloesce.js";

const constructorRegistry: Record<string, new () => any> = {
	DB1Model: DB1Model,
	DB2Model: DB2Model,
	Env: Env
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };