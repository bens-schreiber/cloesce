// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { NullabilityChecks } from "./seed__nullability.cloesce.ts";


import { Env } from "./seed__nullability.cloesce.ts";

const constructorRegistry: Record<string, new () => any> = {
	NullabilityChecks: NullabilityChecks,
	Env: Env
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export {cidl, constructorRegistry}
export default { fetch };