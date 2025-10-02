
import { cloesce } from "cloesce";
import cidl from "./cidl.json";
import { Horse } from "./horse_tinder.cloesce";
import { Like } from "./horse_tinder.cloesce";

const constructorRegistry = {
	Horse: Horse,
	Like: Like
};

export default {
    async fetch(request: Request, env: any, ctx: any): Promise<Response> {
        const instanceRegistry = new Map([
            ["Env", env]
        ]);

        return await cloesce(request, cidl, constructorRegistry, instanceRegistry, { envName: "Env", dbName: "D1_DB" },  "/api");
    }
};
