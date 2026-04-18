import * as clo from "@cloesce/backend.js";
import { CfReadableStream } from "@cloesce/backend.js";
import { HttpResult } from "cloesce";

export const WeatherReport = clo.WeatherReport.impl({});

export const Weather = clo.Weather.impl({
    async uploadPhoto(self, e, s: CfReadableStream) {
        const key = this.Key.photo(self.id);
        await e.bucket.put(key, s);
    },

    downloadPhoto(self) {
        if (!self.photo) {
            return HttpResult.fail(404, "Photo not found");
        }
        return HttpResult.ok(200, self.photo.body);
    }
});

export default {
    async fetch(request: Request, env: clo.Env): Promise<Response> {
        // preflight
        if (request.method === "OPTIONS") {
            return HttpResult.ok(200, undefined, {
                "Access-Control-Allow-Origin": "*",
                "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS",
                "Access-Control-Allow-Headers": "Content-Type, Authorization",
            }).toResponse();
        }

        // Run Cloesce app
        const app = (await clo.cloesce())
            .register(Weather);
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