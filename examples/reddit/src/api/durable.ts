import * as clo from "@cloesce/backend.js";
import { CloesceApp } from "cloesce";
import userDoInitial from "../../migrations/UserDo/1781494217_Initial.js";
import subRedditDoInitial from "../../migrations/SubRedditDo/1781494217_Initial.js";
import { Comment, Post, SubReddit, User } from "./models.js";
import { AuthUser } from "./auth.js";

export class UserDo extends clo.UserDo {
    private app: CloesceApp;
    private sessions: clo.Env["Sessions"];

    constructor(ctx: DurableObjectState, env: clo.Env) {
        super(ctx, env);
        this.sessions = env.Sessions;
        this.app = this.cloesce(env, [userDoInitial]);
        this.app.register(User);
    }

    async fetch(request: Request): Promise<Response> {
        const authUser = await AuthUser.fromRequest(this.sessions, request);
        this.app.register(authUser);

        return await this.app.run(request);
    }

    async appendActivity(update: Partial<clo.Profile>): Promise<void> {
        const current = this.profile.get() ?? { posts: [], comments: [], subReddits: [] };
        const updated = {
            posts: [...current.posts, ...(update.posts ?? [])],
            comments: [...current.comments, ...(update.comments ?? [])],
            subReddits: [...current.subReddits, ...(update.subReddits ?? [])],
        };

        this.profile.put(updated);
    }
}

export class SubRedditDo extends clo.SubRedditDo {
    private app: CloesceApp;
    private sessions: clo.Env["Sessions"];

    constructor(ctx: DurableObjectState, env: clo.Env) {
        super(ctx, env);
        this.sessions = env.Sessions;
        this.app = this.cloesce(env, [subRedditDoInitial]);
        this.app.register(SubReddit).register(Post).register(Comment);
    }

    async fetch(request: Request): Promise<Response> {
        const authUser = await AuthUser.fromRequest(this.sessions, request);
        this.app.register(authUser);

        return await this.app.run(request);
    }

    async setMetadata(metadata: clo.SubRedditMeta): Promise<void> {
        this.metadata.put(metadata);
    }
}
