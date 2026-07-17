import * as clo from "@cloesce/backend.js";
import { AuthUser } from "./auth.js";
import { User } from "./user.js";
import { SubReddit } from "./sub.js";
import { Post, Comment } from "./post.js";


const cors = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, PUT, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Authorization",
};

export default {
  async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
    if (request.method === "OPTIONS") {
      return new Response(null, { headers: cors });
    }

    const app = clo.cloesce(env);
    app.register(User, SubReddit, Post, Comment);

    const cloesceEnv = clo.upgradeEnv(env);
    app.register(await AuthUser.fromRequest(cloesceEnv.Sessions, request));

    const res = await app.run(request);

    // Response headers from a forwarded DO fetch can be immutable, so rebuild.
    return new Response(res.body, {
      status: res.status,
      headers: { ...Object.fromEntries(res.headers), ...cors },
    });
  },
};
