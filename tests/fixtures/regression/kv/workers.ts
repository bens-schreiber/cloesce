// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";

import { JsonKV } from "./seed__kv.cloesce.ts";
import { TextKV } from "./seed__kv.cloesce.ts";


const app = new CloesceApp();
const constructorRegistry = {

};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    return await app.run(request, env, cidl as any, constructorRegistry);
}

export default { fetch };