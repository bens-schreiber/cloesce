import * as clo from "@cloesce/backend.js";
import { CloesceApp, HttpResult } from "cloesce";
import postDoInitial from "../../migrations/PostDo/1784263948_Initial.js";
import { auth, AuthUser } from "./auth.js";
import { SubReddit } from "./sub.js";
import { User } from "./user.js";

export class PostDo extends clo.PostDo {
    private app: CloesceApp;
    private sessions: clo.Env.Sessions;

    constructor(ctx: DurableObjectState, env: clo.CfEnv) {
        super(ctx, env);
        this.sessions = clo.upgradeEnv(env).Sessions;
        this.app = this.cloesce(env, [postDoInitial]);
        this.app.register(Post, Comment);
    }

    async fetch(request: Request): Promise<Response> {
        const authUser = await AuthUser.fromRequest(this.sessions, request);
        this.app.register(authUser);

        return await this.app.run(request);
    }

    async setMeta(meta: clo.PostMeta): Promise<void> {
        this.meta.put(meta);
    }
}

export const Post = clo.Post.impl({
    async create(env, subRedditId, title, content) {
        const username = auth(env);
        if (username instanceof HttpResult) return username;

        if (await SubReddit.Default.get(env, subRedditId) === null) {
            return HttpResult.fail(404, "No such subreddit.");
        }

        const doId = crypto.randomUUID();
        const meta = { title, content, authorName: username, upvotes: 0 };

        const savePost = this.Default.save(env, doId, meta);
        const saveSubReddit = SubReddit.Default.save(env, { posts: [{ postId: doId, subRedditId }] });
        const saveUser = User.Default.save(env, username, { authoredPosts: [{ postId: doId }] });

        const res = await Promise.all([savePost, saveSubReddit, saveUser]);
        const post = res[0].data!;

        return post;
    },

    async vote(self, env, delta) {
        const username = auth(env);
        if (username instanceof HttpResult) return username;

        // A Post's upvotes live in its KV-backed meta, not in SQL.
        const clampDelta = delta >= 0 ? 1 : -1;
        const meta = { ...self.meta, upvotes: self.meta.upvotes + clampDelta };
        env.ctx.meta.put(meta);

        return { ...self, meta };
    },
});

export const Comment = clo.Comment.impl({
    async create(env, postId, content) {
        const username = auth(env);
        if (username instanceof HttpResult) return username;

        const saved = await this.Default.save(env, postId, {
            authorName: username,
            content,
            upvotes: 0,
        });

        const comment = saved.data!;
        await User.Default.save(env, username, { authoredComments: [{ postId, commentId: comment.id }] });

        return comment;
    },

    async vote(self, env, delta) {
        const username = auth(env);
        if (username instanceof HttpResult) return username;

        const clampDelta = delta >= 0 ? 1 : -1;
        const res = await this.Default.save(env, self.doId, {
            ...self,
            upvotes: self.upvotes + clampDelta,
        });

        return res.data!;
    },
});
