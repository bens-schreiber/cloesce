import * as clo from "@cloesce/backend.js";
import { DurableObject } from "cloudflare:workers";
import { HttpResult } from "cloesce";
import userDoInitial from "../../migrations/UserDo/1784326759_Initial.js";
import { newToken } from "./auth.js";

export const user: clo.Api.User.Of = {
  async login(env, username) {
    // Logging in just claims a username
    const token = newToken();
    await env.Sessions.session.put(token, username);

    const found = await env.UserDo.user.get(username);
    const user = found.data ?? {
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
};

export class UserDo extends DurableObject<clo.CfEnv> {
  private base = clo
    .createApp(this, clo.UserDoHost, [userDoInitial])
    .register(clo.User, user)
    .register(clo.AuthoredSubReddit, {})
    .register(clo.AuthoredPost, {})
    .register(clo.AuthoredComment, {});

  async fetch(request: Request): Promise<Response> {
    return this.base.run(request);
  }
}
