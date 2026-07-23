import { createApp, Worker, Weather, type Api, type CfEnv } from "./backend.js";

const weather: Api.Weather.Of = {
  isItRainingSomewhere() {
    return true;
  },

  getCurrentDate() {
    return new Date("2026-01-01T00:00:00.000Z");
  },

  echo(date, isRaining) {
    return { id: 1, date, isRaining };
  },
};

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker).register(Weather, weather).run(request);
  },
};
