import * as clo from "@cloesce/backend.js";
import { DurableObject } from "cloudflare:workers";
import { HttpResult } from "cloesce";
import postDoInitial from "../../migrations/PostDo/1784326759_Initial.js";
import { auth, authFromRequest } from "./auth.js";
import { app } from "./main.js";

export const post = {
  async create(env, subRedditId, title, content) {
    const username = auth(env);
    if (username instanceof HttpResult) {
      return username;
    }

    if (!(await env.SubRedditDb.SubReddit.get(subRedditId)).ok) {
      return HttpResult.fail(404, "No such subreddit.");
    }

    const doId = crypto.randomUUID();
    const meta = { title, content, authorName: username, upvotes: 0 };

    const savePost = env.PostDo.Post.save(doId, { doId, meta });
    const saveSubReddit = env.SubRedditDb.SubReddit.save({
      id: subRedditId,
      posts: [{ postId: doId, subRedditId }],
    });
    const saveUser = env.UserDo.User.save(username, { authoredPosts: [{ postId: doId }] });

    const [saved] = await Promise.all([savePost, saveSubReddit, saveUser]);
    return saved.data!;
  },

  async vote(self, env, delta) {
    const username = auth(env);
    if (username instanceof HttpResult) {
      return username;
    }

    // A Post's upvotes live in its KV-backed meta, not in SQL.
    const clampDelta = delta >= 0 ? 1 : -1;
    const meta = { ...self.meta, upvotes: self.meta.upvotes + clampDelta };
    return env.PostDo.Post.save(self.doId, { meta });
  },
} satisfies clo.Api.Post.Of;

export const comment = {
  async create(env, postId, content) {
    const username = auth(env);
    if (username instanceof HttpResult) {
      return username;
    }

    const saved = await env.PostDo.Comment.save(postId, {
      authorName: username,
      content,
      upvotes: 0,
    });
    if (!saved.ok) {
      return saved;
    }

    const comment = saved.data!;
    await env.UserDo.User.save(username, {
      authoredComments: [{ postId, commentId: comment.id }],
    });

    return comment;
  },

  async vote(self, env, delta) {
    const username = auth(env);
    if (username instanceof HttpResult) {
      return username;
    }

    const clampDelta = delta >= 0 ? 1 : -1;
    return env.PostDo.Comment.save(self.doId, {
      ...self,
      upvotes: self.upvotes + clampDelta,
    });
  },
} satisfies clo.Api.Comment.Of;

export class PostDo extends DurableObject<clo.CfEnv> {
  private base = app().durable(this, [postDoInitial]);

  async fetch(request: Request): Promise<Response> {
    const authed = this.base.register(
      clo.AuthUser,
      await authFromRequest(this.base.env.Sessions, request),
    );

    return authed.run(request);
  }
}
