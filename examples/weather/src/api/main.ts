import { Api, CfEnv, createApp, Worker, Weather, WeatherReport } from "@cloesce/backend.js";
import { HttpResult } from "cloesce";

const weatherReport: Api.WeatherReport.Of = {};

const weather: Api.Weather.Of = {
  async uploadPhoto(self, env, stream) {
    await env.Bucket.photos.put(self.id, stream);
  },

  downloadPhoto(self) {
    if (!self.photo) {
      return HttpResult.fail(404, "Photo not found");
    }
    return HttpResult.ok(200, self.photo.body);
  },
};

const cors = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, PUT, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Authorization",
};

// Exported for use in tests.
export const app = (env: CfEnv) => {
  return createApp(env, Worker).register(Weather, weather).register(WeatherReport, weatherReport);
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    if (request.method === "OPTIONS") {
      return new Response(null, { headers: cors });
    }

    const result = await app(env).run(request);

    for (const [key, value] of Object.entries(cors)) {
      result.headers.set(key, value);
    }
    return result;
  },
};
