import * as clo from "@cloesce/backend.js";
import { DurableObject } from "cloudflare:workers";
import { CloesceApp, HttpResult } from "cloesce";
import userDoInitial from "../../migrations/UserDo/1784326759_Initial.js";
import { AuthUser } from "./auth.js";

export class UserDo extends DurableObject<clo.CfEnv> {
  private app: CloesceApp;
  private sessions: clo.Env.Sessions;

  constructor(ctx: DurableObjectState, env: clo.CfEnv) {
    super(ctx, env);
    this.sessions = clo.upgradeEnv(env).Sessions;
    this.app = clo.cloesce(env, this, [userDoInitial]);
    this.app.register(User);
  }

  async fetch(request: Request): Promise<Response> {
    const authUser = await AuthUser.fromRequest(this.sessions, request);
    this.app.register(authUser);

    return await this.app.run(request);
  }
}

export const User = clo.User.impl({
  async login(env, username) {
    // Logging in just claims a username
    const token = AuthUser.newToken();
    await env.Sessions.session.put(token, username);

    const user = (await this.Default.get(env, username)).data ?? {
      name: username,
      authoredSubReddits: [],
      authoredPosts: [],
      authoredComments: [],
    };

    return { token, user };
  },

  async uploadAvatar(self, env, image) {
    await env.Avatar.avatar.put(self.name, image);
  },

  async downloadAvatar(self, env) {
    const object = await env.Avatar.avatar.get(self.name);
    return object ? HttpResult.ok(200, object.body) : HttpResult.fail(404, "No avatar set.");
  },
});
