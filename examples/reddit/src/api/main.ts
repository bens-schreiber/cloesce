import * as clo from "@cloesce/backend.js";
import { AuthUser } from "./auth.js";
import { Comment, Post, SubReddit, User } from "./models.js";

export { UserDo, SubRedditDo } from "./durable.js";
export { User, SubReddit, Post, Comment } from "./models.js";

const cors = {
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Methods": "GET, POST, PUT, OPTIONS",
    "Access-Control-Allow-Headers": "Content-Type, Authorization",
};

export default {
    async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
        if (request.method === "OPTIONS") return new Response(null, { headers: cors });

        const app = clo.cloesce(env)
            .register(User)
            .register(SubReddit)
            .register(Post)
            .register(Comment);

        const cloesceEnv = clo.upgradeEnv(env);
        app.register(await AuthUser.fromRequest(cloesceEnv.Sessions, request));

        const res = await app.run(request);

        // Response headers from a forwarded DO fetch can be immutable, so rebuild.
        return new Response(res.body, { status: res.status, headers: { ...Object.fromEntries(res.headers), ...cors } });
    },
};
