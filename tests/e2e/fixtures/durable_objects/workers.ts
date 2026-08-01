import { DurableObject } from "cloudflare:workers";
import {
  createApp,
  Global,
  SubReddit,
  Post,
  Comment,
  type Api,
  type CfEnv,
  KValue,
  HttpResult,
} from "./backend.js";
import globalDoInitial from "./migrations/GlobalDo/Initial.js";
import subRedditDoInitial from "./migrations/SubRedditDo/Initial.js";

const global: Api.Global.Of = {
  newGlobal() {
    return { metadata: "default" } as Global;
  },
  getMetadata(self) {
    return self.metadata;
  },
};

const subReddit: Api.SubReddit.Of = {
  newSubReddit() {
    return {
      subId: 0,
      metadata: "default",
      globalMetadata: new KValue({ raw: "default" }),
      posts: [],
    } as SubReddit;
  },
  async feed(self) {
    return self.posts;
  },
};

const custom: Api.Post.Custom = {
  async get(env, id, subId) {
    const key = `Custom/lastGet/${subId}`;
    env.ctx.storage.kv.put(key, id);
    if (env.ctx.storage.kv.get(key) !== id) {
      return HttpResult.fail(500, "injected ctx did not round-trip through shard storage");
    }
    return env.SubRedditDo.Post.get(subId, id);
  },
  list(env, subId) {
    return env.SubRedditDo.Post.list(subId, 0, 100);
  },
  save(env, post, subId) {
    return env.SubRedditDo.Post.save(subId, post);
  },
};

const post: Api.Post.Of = {
  Custom: custom,
};

function app() {
  return createApp()
    .register(Global, global)
    .register(SubReddit, subReddit)
    .register(Post, post)
    .register(Comment, {});
}

export class GlobalDo extends DurableObject<CfEnv> {
  private app = app().durable(this, [globalDoInitial]);

  async fetch(request: Request): Promise<Response> {
    return this.app.run(request);
  }
}

export class SubRedditDo extends DurableObject<CfEnv> {
  private base = app().durable(this, [subRedditDoInitial]);
  async fetch(request: Request): Promise<Response> {
    return this.base.run(request);
  }
}

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return app().worker(env).run(request);
  },
};
