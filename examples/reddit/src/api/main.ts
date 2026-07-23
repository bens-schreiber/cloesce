import * as clo from "@cloesce/backend.js";
import { authFromRequest } from "./auth.js";
import { user } from "./user.js";
import sub from "./sub.js";
import { post, comment } from "./post.js";

export { UserDo } from "./user.js";
export { PostDo } from "./post.js";

const cors = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, PUT, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Authorization",
};

export const app = (env: clo.CfEnv) => {
  return clo
    .createApp(env, clo.Worker)
    .register(clo.User, user)
    .register(clo.SubReddit, sub)
    .register(clo.Post, post)
    .register(clo.Comment, comment)
    .register(clo.AuthoredSubReddit, {})
    .register(clo.AuthoredPost, {})
    .register(clo.AuthoredComment, {})
    .register(clo.SubRedditPost, {});
};

export default {
  async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
    if (request.method === "OPTIONS") {
      return new Response(null, { headers: cors });
    }

    const builder = app(env);
    const withAuth = builder.register(
      clo.AuthUser,
      await authFromRequest(builder.env.Sessions, request),
    );
    const res = await withAuth.run(request);

    // Response headers from a forwarded DO fetch can be immutable, so rebuild.
    return new Response(res.body, {
      status: res.status,
      headers: { ...Object.fromEntries(res.headers), ...cors },
    });
  },
};
