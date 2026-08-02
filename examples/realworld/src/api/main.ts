import * as clo from "@cloesce/backend.js";
import { authFromRequest } from "./auth.js";
import article from "./article.js";
import comment from "./comment.js";
import user from "./user.js";

const cors = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type, Authorization",
};

export function app() {
  return clo
    .createApp()
    .register(clo.User, user)
    .register(clo.Article, article)
    .register(clo.Comment, comment)
    .register(clo.Tag, {})
    .register(clo.Follow, {})
    .register(clo.ArticleTag, {})
    .register(clo.Favorite, {});
}

export { FavoriteDo } from "./article.js";

export default {
  async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
    if (request.method === "OPTIONS") {
      return new Response(null, { headers: cors });
    }

    const builder = app().worker(env);
    const withAuth = builder.register(
      clo.Auth,
      await authFromRequest(builder.env.Db, builder.env.Session, request),
    );
    const res = await withAuth.run(request);

    for (const [name, value] of Object.entries(cors)) {
      res.headers.set(name, value);
    }
    return res;
  },
};
