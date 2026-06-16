import * as clo from "@cloesce/backend.js";
import { CfReadableStream } from "@cloesce/backend.js";
import { HttpResult, KValue } from "cloesce";
import { auth, AuthUser } from "./auth.js";
import { SubRedditDo, UserDo } from "./durable.js";


export const User = clo.User.impl({
    async login(env, username) {
        // Logging in just claims a username: ensure a profile exists, mint a token.
        const profile = env.ctx.profile.get() ?? { posts: [], comments: [], subReddits: [] };
        env.ctx.profile.put(profile);

        const token = AuthUser.newToken();
        await env.Sessions.session.put(token, username);

        // TODO: KValue wrap is a bit awkward here
        const user = { username, profile: new KValue("profile", profile) } as clo.User.Self;
        return { token, user };
    },

    async uploadAvatar(self, env, image: CfReadableStream) {
        await env.Avatars.avatar.put(self.username, image);
    },

    downloadAvatar: (self) =>
        self.avatar ? HttpResult.ok(200, self.avatar.body) : HttpResult.fail(404, "No avatar set."),
});

export const SubReddit = clo.SubReddit.impl({
    create: (env, meta) =>
        auth(env, async (username) => {
            const subId = crypto.randomUUID();
            await env.SubRedditDo.instance<SubRedditDo>(subId).setMetadata(meta);
            await env.UserDo.instance<UserDo>(username).appendActivity({ subReddits: [subId] });

            const entry: clo.SubRedditEntry = { subId, name: meta.name };
            await env.SubReddits.put(env.SubReddits.entry.template(subId), JSON.stringify(entry));

            return { subId, metadata: new KValue("metadata", meta) } as clo.SubReddit.Self;
        }),

    async list(env) {
        const { keys } = await env.SubReddits.list({ prefix: env.SubReddits.directory.template() });
        const results = await Promise.all(keys.map((k) => env.SubReddits.get(k.name, "json"))) as clo.SubRedditEntry[];
        return { results, cursor: null, complete: true };
    },

    feed: async (_self, env, subId) =>
        (await clo.Post.GeneratedSource.Default.list(env, subId, 0, 100)).data ?? [],
});

export const Post = clo.Post.impl({
    create: (env, subId, title, content) =>
        auth(env, async (username) => {
            const saved = await clo.Post.Orm.save(env.ctx, { subId, author: username, title, content, upvotes: 0 });
            await env.UserDo.instance<UserDo>(username).appendActivity({ posts: [saved.value!.id] });
            return saved.value!;
        }),

    vote: (self, env, subId, delta) =>
        auth(env, async () => {
            const clampDelta = (delta >= 0 ? 1 : -1);
            const res = await clo.Post.Orm.save(env.ctx, { ...self, upvotes: self.upvotes + clampDelta });
            return res.value!;
        }),
});

export const Comment = clo.Comment.impl({
    create: (env, subId, postId, content) =>
        auth(env, async (username) => {
            const saved = await clo.Comment.Orm.save(env.ctx, { subId, postId, author: username, content, upvotes: 0 });
            await env.UserDo.instance<UserDo>(username).appendActivity({ comments: [saved.value!.id] });
            return saved.value!;
        }),

    vote: (self, env, subId, delta) =>
        auth(env, async () => {
            const clampDelta = (delta >= 0 ? 1 : -1);
            const res = await clo.Comment.Orm.save(env.ctx, { ...self, upvotes: self.upvotes + clampDelta });
            return res.value!;
        }),
});
