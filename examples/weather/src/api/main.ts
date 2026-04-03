import { ExecutionContext, ReadableStream } from "@cloudflare/workers-types";
import { cloesce, Env, Weather } from "@cloesce/backend";
import { HttpResult } from "cloesce";

class WeatherApi extends Weather.Api {
    async uploadPhoto(self: Weather.Self, e: Env, s: ReadableStream): Promise<HttpResult<void>> {
        await e.bucket.put(`weather/photo/${self.id}`, s);
        return HttpResult.ok(200);
    }

    downloadPhoto(self: Weather.Self): HttpResult<ReadableStream> {
        if (!self.photo) {
            return HttpResult.fail(404, "Photo not found");
        }
        return HttpResult.ok(200, self.photo.body);
    }
}

export default async function fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    // Run Cloesce app
    const app = (await cloesce())
        .register(new WeatherApi());

    const result = await app.run(request, env);
    return result;
}