import { type Api, HttpResult } from "@cloesce/backend.js";
import { auth } from "./auth.js";

export default {
  async create(env, title, description) {
    const username = auth(env);
    if (username instanceof HttpResult) {
      return username;
    }

    const sub = await env.SubRedditDb.subReddit.save({ title, description, posts: [] });
    if (!sub.ok) {
      return sub;
    }

    await env.UserDo.user.save(username, {
      authoredSubReddits: [{ subRedditId: sub.data!.id }],
    });

    return sub.data!;
  },

  async feed(self, env) {
    const full = await env.SubRedditDb.subReddit.load(self, {
      posts: {
        post: {
          meta: {},
          comments: {},
        },
      },
    });
    return full.data?.posts.map((p) => p.post) ?? [];
  },
} satisfies Api.SubReddit.Of;
