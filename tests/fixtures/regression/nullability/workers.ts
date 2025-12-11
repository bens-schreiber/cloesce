// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { NullabilityChecks } from "./seed__nullability.cloesce.ts";


const app = new CloesceApp();
const constructorRegistry = {
	NullabilityChecks: NullabilityChecks
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    return await app.run(request, env, cidl as any, constructorRegistry);
}

export default { fetch };