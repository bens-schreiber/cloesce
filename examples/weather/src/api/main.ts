import * as Cloesce from "@cloesce/backend.js";
import { CfReadableStream } from "@cloesce/backend.js";
import { HttpResult } from "cloesce";


export class Weather extends Cloesce.Weather.Api {
    async uploadPhoto(self: Cloesce.Weather.Self, e: Cloesce.Env, s: CfReadableStream): Promise<void> {
        const key = Cloesce.Weather.KeyFormat.photo(self.id);
        await e.bucket.put(key, s);
    }

    downloadPhoto(self: Cloesce.Weather.Self): HttpResult<CfReadableStream> {
        if (!self.photo) {
            return HttpResult.fail(404, "Photo not found");
        }
        return HttpResult.ok(200, self.photo.body);
    }
}

export default {
    async fetch(request: Request, env: Cloesce.Env): Promise<Response> {
        // preflight
        if (request.method === "OPTIONS") {
            return HttpResult.ok(200, undefined, {
                "Access-Control-Allow-Origin": "*",
                "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS",
                "Access-Control-Allow-Headers": "Content-Type, Authorization",
            }).toResponse();
        }

        // Run Cloesce app
        const app = (await Cloesce.cloesce())
            .register(new Weather());
        const result = await app.run(request, env);

        // attach CORS headers
        result.headers.set("Access-Control-Allow-Origin", "*");
        result.headers.set(
            "Access-Control-Allow-Methods",
            "GET, POST, PUT, DELETE, OPTIONS"
        );
        result.headers.set(
            "Access-Control-Allow-Headers",
            "Content-Type, Authorization"
        );

        return result;
    }
};