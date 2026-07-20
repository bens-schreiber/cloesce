import { DurableObject } from "cloudflare:workers";
import { DurableObjectState } from "@cloudflare/workers-types";
import { CloesceApp } from "cloesce";
import * as clo from "./backend.js";
import globalDoInitial from "./migrations/GlobalDo/Initial.js";
import subRedditDoInitial from "./migrations/SubRedditDo/Initial.js";

const Global = clo.Global.impl({
  newGlobal() {
    return { metadata: "default" } as clo.Global.Self;
  },

  getMetadata(self) {
    return self.metadata;
  },
});

const SubReddit = clo.SubReddit.impl({
  newSubReddit() {
    // mock, return a default
    return {
      subId: 0,
      metadata: "default",
      globalMetadata: { raw: "default" },
    } as clo.SubReddit.Self;
  },

  async feed(self) {
    return self.posts;
  },
});

const PostCustomDs = clo.Post.Custom.impl({
  async get(env, id, subId) {
    return await Post.Default.get(env, subId, id);
  },

  async list(env, subId) {
    return await Post.Default.list(env, subId, 0, 100);
  },

  async save(env, post, subId) {
    return await Post.Default.save(env, subId, post);
  },
});

const Post = clo.Post.impl({
  Custom: PostCustomDs,
});

const Comment = clo.Comment.impl({});

export class GlobalDo extends DurableObject<clo.CfEnv> {
  private app: CloesceApp;

  constructor(ctx: DurableObjectState, env: clo.CfEnv) {
    super(ctx, env);
    this.app = clo.cloesce(env, this, [globalDoInitial]);
    this.app.register(Global);
  }

  async fetch(request: Request): Promise<Response> {
    return await this.app.run(request);
  }
}

export class SubRedditDo extends DurableObject<clo.CfEnv> {
  private app: CloesceApp;

  constructor(ctx: DurableObjectState, env: clo.CfEnv) {
    super(ctx, env);
    this.app = clo.cloesce(env, this, [subRedditDoInitial]);
    this.app.register(SubReddit, Post, Comment);
  }

  async fetch(request: Request): Promise<Response> {
    return await this.app.run(request);
  }
}

export default {
  async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
    const app = clo.cloesce(env);
    app.register(Global, SubReddit, Post, Comment);
    return await app.run(request);
  },
};
