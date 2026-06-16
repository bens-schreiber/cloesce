import * as clo from "@cloesce/backend.js";
import { HttpResult } from "cloesce";

export const WeatherReport = clo.WeatherReport.impl({});

export const Weather = clo.Weather.impl({
  async uploadPhoto(self, env, stream) {
    await env.Bucket.photos.put(self.id, stream);
  },

  downloadPhoto(self) {
    if (!self.photo) {
      return HttpResult.fail(404, "Photo not found");
    }
    return HttpResult.ok(200, self.photo.body);
  },
});

const cors = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, PUT, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Authorization",
};

export default {
  async fetch(request: Request, env: clo.Env): Promise<Response> {
    if (request.method === "OPTIONS") {
      return new Response(null, { headers: cors });
    }

    const app = clo.cloesce(env);
    app.register(Weather, WeatherReport);

    const result = await app.run(request);

    for (const [key, value] of Object.entries(cors)) {
      result.headers.set(key, value);
    }
    return result;
  },
};
