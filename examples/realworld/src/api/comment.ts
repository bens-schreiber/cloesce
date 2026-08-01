import { Api, Comment } from "@cloesce/backend.js";
import { HttpResult } from "cloesce";
import { requireAuth } from "./auth.js";

const Default = {
  async list(env, articleId) {
    const rows = await env.db
      .prepare(`SELECT * FROM "Comment" WHERE "articleId" = ?1
                ORDER BY "createdAt" DESC, "id" DESC`)
      .bind(articleId)
      .all<Comment>();

    return HttpResult.ok(200, rows.results);
  },
} satisfies Api.Comment.Default;

export default {
  Default,

  async create(env, articleId, body) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    const now = new Date().toISOString();
    return env.db.comment.save({
      body,
      articleId,
      authorId: me.id,
      createdAt: now,
      updatedAt: now,
    });
  },

  async update(self, env, comment) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    if (self.authorId !== me.id) {
      return HttpResult.fail(403, "You may only edit your own comments.");
    }

    return await env.db.comment.save({
      ...self,
      ...comment,
      updatedAt: new Date().toISOString(),
    });
  },

  async del(self, env) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }
    if (self.authorId !== me.id) {
      return HttpResult.fail(403, "You may only delete your own comments.");
    }

    await env.db.prepare(`DELETE FROM "Comment" WHERE "id" = ?1`).bind(self.id).run();
  },
} satisfies Api.Comment.Of;
