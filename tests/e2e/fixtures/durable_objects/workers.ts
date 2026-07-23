import { DurableObject } from "cloudflare:workers";
import {
  createApp,
  Worker,
  GlobalDoHost,
  SubRedditDoHost,
  Global,
  SubReddit,
  Post,
  Comment,
  type Api,
  type CfEnv,
  KValue,
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
  get(env, id, subId) {
    return env.SubRedditDo.post.get(subId, id);
  },
  list(env, subId) {
    return env.SubRedditDo.post.list(subId, 0, 100);
  },
  save(env, post, subId) {
    return env.SubRedditDo.post.save(subId, post);
  },
};

const post: Api.Post.Of = {
  Custom: custom,
};

export class GlobalDo extends DurableObject<CfEnv> {
  private app = createApp(this, GlobalDoHost, [globalDoInitial]).register(Global, global);

  async fetch(request: Request): Promise<Response> {
    return this.app.run(request);
  }
}

export class SubRedditDo extends DurableObject<CfEnv> {
  private base = createApp(this, SubRedditDoHost, [subRedditDoInitial])
    .register(SubReddit, subReddit)
    .register(Post, post)
    .register(Comment, {});
  async fetch(request: Request): Promise<Response> {
    return this.base.run(request);
  }
}

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker)
      .register(Global, global)
      .register(SubReddit, subReddit)
      .register(Post, post)
      .register(Comment, {})
      .run(request);
  },
};
