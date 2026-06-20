import * as clo from "@cloesce/backend.js";
import { HttpResult } from "cloesce";
import { AuthUser, auth } from "./auth.js";
import { SubRedditDo, UserDo } from "./durable.js";

export const User = clo.User.impl({
  async login(env, username) {
    // Logging in just claims a username: ensure a profile exists, mint a token.
    const profile = env.ctx.profile.get() ?? { posts: [], comments: [], subReddits: [] };
    env.ctx.profile.put(profile);

    const token = AuthUser.newToken();
    await env.Sessions.session.put(token, username);

    const user = { username, profile } as clo.User.Self;
    return { token, user };
  },

  async uploadAvatar(self, env, image) {
    await env.Avatars.avatar.put(self.username, image);
  },

  downloadAvatar: (self) =>
    self.avatar ? HttpResult.ok(200, self.avatar.body) : HttpResult.fail(404, "No avatar set."),
});

export const SubReddit = clo.SubReddit.impl({
  async create(env, meta) {
    const username = auth(env);
    if (username instanceof HttpResult) return username;

    const subId = crypto.randomUUID();
    await env.SubRedditDo.stub<SubRedditDo>(subId).setMetadata(meta);
    await env.UserDo.stub<UserDo>(username).appendActivity({ subReddits: [subId] });

    const entry = { subId, name: meta.name };
    await env.SubReddits.put(env.SubReddits.entry.template(subId), JSON.stringify(entry));

    return { subId, metadata: meta };
  },

  async list(env) {
    const { keys } = await env.SubReddits.entry.list();
    return (await Promise.all(
      keys.map((k) => env.SubReddits.get(k.name, "json")),
    )) as clo.SubRedditEntry[];
  },

  async feed(self, env, subId) {
    const res = (await Post.Default.list(env, subId, 0, 100)).data ?? [];
    return res;
  },
});

export const Post = clo.Post.impl({
  async create(env, subId, title, content) {
    const username = auth(env);
    if (username instanceof HttpResult) return username;

    const saved = await this.Default.save(env, subId, {
      author: username,
      title,
      content,
      upvotes: 0,
    });

    await env.UserDo.stub<UserDo>(username).appendActivity({ posts: [saved.data!.id] });
    return saved.data!;
  },

  async vote(self, env, subId, delta) {
    const username = auth(env);
    if (username instanceof HttpResult) return username;

    const clampDelta = delta >= 0 ? 1 : -1;
    const res = await this.Default.save(env, subId, {
      ...self,
      upvotes: self.upvotes + clampDelta,
    });
    return res.data!;
  },
});

export const Comment = clo.Comment.impl({
  async create(env, subId, postId, content) {
    const username = auth(env);
    if (username instanceof HttpResult) return username;

    const saved = await this.Default.save(env, subId, {
      subId,
      postId,
      author: username,
      content,
      upvotes: 0,
    });
    await env.UserDo.stub<UserDo>(username).appendActivity({ comments: [saved.data!.id] });
    return saved.data!;
  },

  async vote(self, env, subId, delta) {
    const username = auth(env);
    if (username instanceof HttpResult) return username;

    const clampDelta = delta >= 0 ? 1 : -1;
    const res = await this.Default.save(env, subId, {
      ...self,
      upvotes: self.upvotes + clampDelta,
    });
    return res.data!;
  },
});
