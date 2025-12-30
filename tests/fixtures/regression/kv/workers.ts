// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";

import { Data } from "./seed__kv.cloesce.ts";
import { DataScientist } from "./seed__kv.cloesce.ts";
import { JsonValue } from "./seed__kv.cloesce.ts";
import { StreamValue } from "./seed__kv.cloesce.ts";
import { TextValue } from "./seed__kv.cloesce.ts";
import { DataValue } from "./seed__kv.cloesce.ts";

const app = new CloesceApp();
const constructorRegistry = {
	Data: Data,
	DataScientist: DataScientist,
	JsonValue: JsonValue,
	StreamValue: StreamValue,
	TextValue: TextValue,
	DataValue: DataValue
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    return await app.run(request, env, cidl as any, constructorRegistry);
}

export default { fetch };