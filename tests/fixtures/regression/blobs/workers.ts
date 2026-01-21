// GENERATED CODE. DO NOT MODIFY.
import { CloesceApp } from "cloesce/backend";
import cidl from "./cidl.json";
import { BlobHaver } from "./seed__blobs.cloesce.ts";

import { BlobService } from "./seed__blobs.cloesce.ts";

const constructorRegistry: Record<string, new () => any> = {
	BlobHaver: BlobHaver,
	BlobService: BlobService
};

async function fetch(request: Request, env: any, ctx: any): Promise<Response> {
    const app = await CloesceApp.init(cidl as any, constructorRegistry);
    return await app.run(request, env);
}

export default { fetch };