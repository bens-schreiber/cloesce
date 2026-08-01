import { User, Api, Article } from "@cloesce/backend.js";
import { HttpResult } from "cloesce";
import { profileDto, requireAuth, userDto } from "./auth.js";

const ByName = {
  async get(env, name) {
    const user = await env.Db.prepare(`SELECT * FROM "User" WHERE "username" = ?1`)
      .bind(name)
      .first<User>();
    if (!user) {
      return HttpResult.fail(404, `No user named "${name}".`);
    }

    return HttpResult.ok(200, user);
  },
} satisfies Api.User.ByName;

export default {
  ByName,

  async register(env, username, email, password) {
    const user = await env.Db.User.ByName.get(username);
    if (user.ok) {
      return HttpResult.fail(422, `Username "${username}" is already taken.`);
    }

    const saved = await env.Db.User.save({
      username,
      email,
      bio: "",
      image: null,
      passwordHash: password,
    });

    return saved.ok ? userDto(saved.data!) : HttpResult.fail(saved.status, saved.message);
  },

  async login(env, email, password) {
    const user = await env.Db.prepare(`SELECT * FROM "User" WHERE "email" = ?1`)
      .bind(email)
      .first<User>();
    if (!user || user.passwordHash !== password) {
      return HttpResult.fail(401, "Invalid email or password.");
    }

    return userDto(user);
  },

  async update(env, user) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    const saved = await env.Db.User.save({
      id: me.id,
      username: user.username ?? me.username,
      email: user.email ?? me.email,
      bio: user.bio ?? me.bio,
      image: user.image === undefined ? me.image : user.image,
    });

    return saved.ok ? userDto(saved.data!) : HttpResult.fail(saved.status, saved.message);
  },

  async profile(env, username) {
    const them = await env.Db.User.ByName.get(username);
    if (!them.ok) {
      return HttpResult.fail(them.status, them.message);
    }

    const me = env.Auth.user;
    const following = me ? (await env.Db.Follow.get(me.id, them.data!.id)).ok : false;

    return profileDto(them.data!, following);
  },

  async follow(env, username) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    const them = await env.Db.User.ByName.get(username);
    if (!them.ok) {
      return HttpResult.fail(them.status, them.message);
    }
    if (them.data!.id === me.id) {
      return HttpResult.fail(422, "You cannot follow yourself.");
    }

    const saved = await env.Db.Follow.save({ followerId: me.id, followeeId: them.data!.id });
    if (!saved.ok) {
      return HttpResult.fail(saved.status, saved.message);
    }

    return profileDto(them.data!, true);
  },

  async unfollow(env, username) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    const them = await env.Db.User.ByName.get(username);
    if (!them.ok) {
      return HttpResult.fail(them.status, them.message);
    }

    await env.Db.prepare(`DELETE FROM "Follow" WHERE "followerId" = ?1 AND "followeeId" = ?2`)
      .bind(me.id, them.data!.id)
      .run();

    return profileDto(them.data!, false);
  },

  async feed(env, limit, offset) {
    const me = requireAuth(env);
    if (me instanceof HttpResult) {
      return me;
    }

    const rows = await env.Db.prepare(`SELECT a.* FROM "Article" a
                JOIN "Follow" f ON f."followeeId" = a."authorId"
                WHERE f."followerId" = ?1
                ORDER BY a."createdAt" DESC, a."id" DESC
                LIMIT ?2 OFFSET ?3`)
      .bind(me.id, limit ?? 20, offset ?? 0)
      .all<Article>();

    return rows.results.length ? env.Db.Article.hydrateAll(rows.results) : HttpResult.ok(200, []);
  },

  current(env) {
    const me = requireAuth(env);
    return me instanceof HttpResult ? me : userDto(me);
  },
} satisfies Api.User.Of;
