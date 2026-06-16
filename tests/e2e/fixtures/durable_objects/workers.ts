import { DurableObjectState } from "@cloudflare/workers-types";
import { CloesceApp, HttpResult, DeepPartial } from "cloesce";
import * as clo from "./backend.js";
import globalDoInitial from "./migrations/GlobalDo/Initial.js";
import subRedditDoInitial from "./migrations/SubRedditDo/Initial.js";

const Global = clo.Global.impl({
  newGlobal() {
    return { metadata: { raw: "default" } } as clo.Global.Self;
  },

  getMetadata(self) {
    return self.metadata.value;
  },
});

const SubReddit = clo.SubReddit.impl({
  newSubReddit() {
    return {
      subId: 0,
      metadata: { raw: "default" },
      globalMetadata: { raw: "default" },
    } as clo.SubReddit.Self;
  },

  async feed(self, env, subId) {
    const res = await clo.Post.GeneratedSource.Default.list(env, subId, 0, 100);
    return res.data ?? [];
  },
});

const PostCustomDs = clo.Post.Custom.impl({
  async get(env, id, subId) {
    const row = await env.SubRedditDo.instance<SubRedditDo>(subId).getPost(subId, id);
    return row === null ? HttpResult.fail(404) : HttpResult.ok(200, row);
  },

  async list(env, subId) {
    const rows = await env.SubRedditDo.instance<SubRedditDo>(subId).listPosts(subId);
    return HttpResult.ok(200, rows);
  },

  async save(env, post, subId) {
    const row = await env.SubRedditDo.instance<SubRedditDo>(subId).savePost(subId, post);
    return row === null ? HttpResult.fail(404) : HttpResult.ok(200, row);
  },
});

const Post = clo.Post.impl({
  Custom: PostCustomDs,
});

const Comment = clo.Comment.impl({});

export class GlobalDo extends clo.GlobalDo {
  private app: CloesceApp;

  constructor(ctx: DurableObjectState, env: clo.CfEnv) {
    super(ctx, env);
    this.app = this.cloesce(env, [globalDoInitial]);
    this.app.register(Global);
  }

  async fetch(request: Request): Promise<Response> {
    return await this.app.run(request);
  }
}

export class SubRedditDo extends clo.SubRedditDo {
  private app: CloesceApp;

  constructor(ctx: DurableObjectState, env: clo.CfEnv) {
    super(ctx, env);
    this.app = this.cloesce(env, [subRedditDoInitial]);
    this.app.register(SubReddit, Post, Comment);
  }

  async fetch(request: Request): Promise<Response> {
    return await this.app.run(request);
  }

  // Manual implementations for the custom data source
  // (which does not inject a DO instance in the schema)
  async getPost(subId: number, postId: number): Promise<clo.Post.Self | null> {
    return (await Post.Default.get({ ctx: this }, subId, postId)).data ?? null;
  }

  async listPosts(subId: number): Promise<clo.Post.Self[]> {
    return (await Post.Default.list({ ctx: this }, subId, 0, 100)).data ?? [];
  }

  async savePost(subId: number, post: DeepPartial<clo.Post.Self>): Promise<clo.Post.Self | null> {
    return (await Post.Default.save({ ctx: this }, subId, post)).data ?? null;
  }
}

export default {
  async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
    const app = clo.cloesce(env);
    app.register(Global, SubReddit, Post, Comment);
    return await app.run(request);
  },
};
