import * as clo from "@cloesce/backend.js";
import { DurableObject } from "cloudflare:workers";
import { HttpResult } from "cloesce";
import userDoInitial from "../../migrations/UserDo/1784326759_Initial.js";
import { authFromRequest, newToken } from "./auth.js";
import { app } from "./main.js";

export const user = {
  async login(env, username) {
    // Logging in just claims a username
    const token = newToken();
    await env.sessions.session.put(token, username);

    const found = await env.userDo.user.get(username);
    const user = found.data ?? {
      name: username,
      authoredSubReddits: [],
      authoredPosts: [],
      authoredComments: [],
    };

    return { token, user };
  },

  async uploadAvatar(self, env, image) {
    await env.avatar.avatar.put(self.name, image);
  },

  async downloadAvatar(self, env) {
    const object = await env.avatar.avatar.get(self.name);
    return object ? HttpResult.ok(200, object.body) : HttpResult.fail(404, "No avatar set.");
  },
} satisfies clo.Api.User.Of;

export class UserDo extends DurableObject<clo.CfEnv> {
  private base = app().durable(this, [userDoInitial]);

  async fetch(request: Request): Promise<Response> {
    const authed = this.base.register(
      clo.AuthUser,
      await authFromRequest(this.base.env.sessions, request),
    );

    return authed.run(request);
  }
}
