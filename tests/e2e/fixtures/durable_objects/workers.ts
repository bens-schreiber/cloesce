import { DurableObjectState } from "@cloudflare/workers-types";
import { CloesceApp } from "cloesce";
import * as clo from "./backend.js";

const Leaderboard = clo.Leaderboard.impl({
  async setScore(env, tenantId, score) {
    env.$ctx.score.put(score);
  },
  getScore(env) {
    return env.$ctx.score.get() ?? 0;
  },
});

const LeaderboardEntry = clo.LeaderboardEntry.impl({});

const Global = clo.Global.impl({
  setConfig(env, value) {
    env.$ctx.config.put(value);
  },
  getConfig(env) {
    return env.$ctx.config.get();
  },
});

export class LeaderboardDo extends clo.LeaderboardDo {
  private app: CloesceApp;

  constructor(ctx: DurableObjectState, env: clo.Env) {
    super(ctx, env);
    this.app = this.cloesce(env);
    this.app.register(Leaderboard);
    this.app.register(LeaderboardEntry);
  }

  async fetch(request: Request): Promise<Response> {
    return await this.app.run(request);
  }
}

export class GlobalDo extends clo.GlobalDo {
  private app: CloesceApp;

  constructor(ctx: DurableObjectState, env: clo.Env) {
    super(ctx, env);
    this.app = this.cloesce(env);
    this.app.register(Global);
  }

  async fetch(request: Request): Promise<Response> {
    return await this.app.run(request);
  }
}

export default {
  async fetch(request: Request, env: clo.Env): Promise<Response> {
    const app = clo.cloesce(env);
    return await app.run(request);
  },
};
