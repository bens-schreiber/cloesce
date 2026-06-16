import * as clo from "./backend.js";

const Weather = clo.Weather.impl({
  isItRainingSomewhere() {
    return true;
  },

  getCurrentDate() {
    return new Date("2026-01-01T00:00:00.000Z");
  },

  echo(date, isRaining) {
    return {
      id: 1,
      date,
      isRaining,
    };
  },
});

export default {
  async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
    const app = clo.cloesce(env);
    app.register(Weather);
    return await app.run(request);
  },
};
