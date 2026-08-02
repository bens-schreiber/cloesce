import { Article, Api, CfEnv, Auth, Env } from "@cloesce/backend.js";
import { DurableObject } from "cloudflare:workers";
import { HttpResult } from "cloesce";
import favoriteDoInitial from "../../migrations/FavoriteDo/1785535428_Initial.js";
import { authFromRequest, requireAuth } from "./auth.js";
import { app } from "./main.js";

const Default = {
  async get(env, slug) {
    const row = await env.Db.prepare(`SELECT * FROM "Article" WHERE "slug" = ?1`)
      .bind(slug)
      .first<Article>();

    if (!row) {
      return HttpResult.fail(404, `No article with slug "${slug}".`);
    }

    return env.Db.Article.hydrate(row);
  },

  async list(env, tag, author, favorited, limit, offset) {
    const rows = await env.Db.prepare(
      `
        SELECT a.*
        FROM "Article" a
        WHERE (
            ?1 IS NULL
            OR a."id" IN (
              SELECT at."articleId"
              FROM "ArticleTag" at
              JOIN "Tag" t ON t."id" = at."tagId"
              WHERE t."name" = ?1
            )
          )
          AND (
            ?2 IS NULL
            OR a."authorId" IN (SELECT "id" FROM "User" WHERE "username" = ?2)
          )
          AND (
            ?3 IS NULL
            OR a."id" IN (
              SELECT f."articleId"
              FROM "Favorite" f
              JOIN "User" u ON u."id" = f."userId"
              WHERE u."username" = ?3
            )
          )
        ORDER BY a."createdAt" DESC, a."id" DESC
        LIMIT ?4 OFFSET ?5
        `,
    )
      .bind(tag, author, favorited, limit ?? 20, offset ?? 0)
      .all<Article>();

    return rows.results.length ? env.Db.Article.hydrateAll(rows.results) : HttpResult.ok(200, []);
  },
} satisfies Api.Article.Default;

const InFavoriteDo = {
  async get(env, id) {
    const row = await env.Db.prepare(`SELECT * FROM "Article" WHERE "id" = ?1`)
      .bind(id)
      .first<Article>();

    if (!row) {
      return HttpResult.fail(404, `No article ${id}.`);
    }

    return env.Db.Article.InFavoriteDo.hydrate(row);
  },
} satisfies Api.Article.InFavoriteDo;

export default {
  Default,
  InFavoriteDo,

  async create(env, title, description, body, tags) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    const now = new Date().toISOString();
    const slug = crypto.randomUUID();

    return env.Db.Article.save({
      title,
      description,
      body,
      slug,
      authorId: me.id,
      createdAt: now,
      updatedAt: now,
      tags: [...new Set(tags)].map((name) => ({ tag: { name } })),
      favoriteCount: 0,
    });
  },

  async update(self, env, update) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    if (self.authorId !== me.id) {
      return HttpResult.fail(403, "You may only edit your own articles.");
    }

    return env.Db.Article.save({
      ...self,
      ...update,
      updatedAt: new Date().toISOString(),
    });
  },

  async del(self, env) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    if (self.authorId !== me.id) {
      return HttpResult.fail(403, "You may only delete your own articles.");
    }

    await env.Db.batch([
      env.Db.prepare(`DELETE FROM "ArticleTag" WHERE "articleId" = ?1`).bind(self.id),
      env.Db.prepare(`DELETE FROM "Favorite" WHERE "articleId" = ?1`).bind(self.id),
      env.Db.prepare(`DELETE FROM "Comment" WHERE "articleId" = ?1`).bind(self.id),
      env.Db.prepare(`DELETE FROM "Article" WHERE "id" = ?1`).bind(self.id),
    ]);

    env.FavoriteDo.count.put(env.ctx, 0);
  },

  async favorite(self, env) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    const alreadyFavorited = await env.Db.Favorite.get(me.id, self.id);
    if (alreadyFavorited.ok) {
      return;
    }

    const saved = await env.Db.Favorite.save({ userId: me.id, articleId: self.id });
    if (!saved.ok) {
      return HttpResult.fail(saved.status, saved.message);
    }

    bumpFavoriteCount(env, +1);
  },

  async unfavorite(self, env) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    const removed = await env.Db.prepare(
      `DELETE FROM "Favorite" WHERE "userId" = ?1 AND "articleId" = ?2`,
    )
      .bind(me.id, self.id)
      .run();

    if (removed.meta.changes) {
      bumpFavoriteCount(env, -1);
    }
  },
} satisfies Api.Article.Of;

function bumpFavoriteCount(env: Env.ArticleFavorite | Env.ArticleUnfavorite, delta: number) {
  const current = env.FavoriteDo.count.get(env.ctx) ?? 0;
  env.FavoriteDo.count.put(env.ctx, current + delta);
}

export class FavoriteDo extends DurableObject<CfEnv> {
  private base = app().durable(this, [favoriteDoInitial]);

  async fetch(request: Request): Promise<Response> {
    const authed = this.base.register(
      Auth,
      await authFromRequest(this.base.env.Db, this.base.env.Session, request),
    );

    return authed.run(request);
  }
}
