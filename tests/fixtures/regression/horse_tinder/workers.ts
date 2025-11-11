// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { Horse } from "./seed__horse_tinder.cloesce.ts";
import { Like } from "./seed__horse_tinder.cloesce.ts";

const app = new CloesceApp();
const constructorRegistry = {
	Horse: Horse,
	Like: Like
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const envMeta = { envName: "Env", dbName: "db" };
    return await app.run(request, env, cidl as any, constructorRegistry, envMeta);
}

export default { fetch };