import * as clo from "@cloesce/backend.js";
import { DurableObject } from "cloudflare:workers";
import { HttpResult } from "cloesce";
import postDoInitial from "../../migrations/PostDo/1784326759_Initial.js";
import { auth, authFromRequest } from "./auth.js";

export const post: clo.Api.Post.Of = {
  async create(env, subRedditId, title, content) {
    const username = auth(env);
    if (username instanceof HttpResult) {
      return username;
    }

    if (!(await env.SubRedditDb.subReddit.get(subRedditId)).ok) {
      return HttpResult.fail(404, "No such subreddit.");
    }

    const doId = crypto.randomUUID();
    const meta = { title, content, authorName: username, upvotes: 0 };

    const savePost = env.PostDo.post.save(doId, { doId, meta });
    const saveSubReddit = env.SubRedditDb.subReddit.save({
      id: subRedditId,
      posts: [{ postId: doId, subRedditId }],
    });
    const saveUser = env.UserDo.user.save(username, { authoredPosts: [{ postId: doId }] });

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
    return env.PostDo.post.save(self.doId, { meta });
  },
};

export const comment: clo.Api.Comment.Of = {
  async create(env, postId, content) {
    const username = auth(env);
    if (username instanceof HttpResult) {
      return username;
    }

    const saved = await env.PostDo.comment.save(postId, {
      authorName: username,
      content,
      upvotes: 0,
    });
    if (!saved.ok) {
      return saved;
    }

    const comment = saved.data!;
    await env.UserDo.user.save(username, {
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
    return env.PostDo.comment.save(self.doId, {
      ...self,
      upvotes: self.upvotes + clampDelta,
    });
  },
};

export class PostDo extends DurableObject<clo.CfEnv> {
  private base = clo
    .createApp(this, clo.PostDoHost, [postDoInitial])
    .register(clo.Post, post)
    .register(clo.Comment, comment);

  async fetch(request: Request): Promise<Response> {
    const app = this.base.register(
      clo.AuthUser,
      await authFromRequest(this.base.env.Sessions, request),
    );
    return app.run(request);
  }
}
