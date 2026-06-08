import { DurableObjectState } from "@cloudflare/workers-types";
import * as clo from "./backend.js";

export class LeaderboardDo extends clo.LeaderboardDo {
  constructor(state: DurableObjectState, env: clo.Env) {
    super(state, env);
  }

  async fetch(_request: Request): Promise<Response> {
    return new Response("not implemented", { status: 501 });
  }
}

export class GlobalDo extends clo.GlobalDo {
  constructor(state: DurableObjectState, env: clo.Env) {
    super(state, env);
  }

  async fetch(_request: Request): Promise<Response> {
    return new Response("not implemented", { status: 501 });
  }
}

export default {
  async fetch(request: Request, env: clo.Env): Promise<Response> {
    const app = await clo.cloesce();
    return await app.run(request, env);
  },
};
