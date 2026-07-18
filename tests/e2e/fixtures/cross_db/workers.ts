import { DurableObject } from "cloudflare:workers";
import { DurableObjectState } from "@cloudflare/workers-types";
import { CloesceApp } from "cloesce";
import * as clo from "./backend.js";
import libraryDoInitial from "./migrations/LibraryDo/Initial.js";

const Author = clo.Author.impl({});
const Book = clo.Book.impl({});

export class LibraryDo extends DurableObject<clo.CfEnv> {
  private app: CloesceApp;

  constructor(ctx: DurableObjectState, env: clo.CfEnv) {
    super(ctx, env);
    this.app = clo.cloesce(env, this, [libraryDoInitial]);
    this.app.register(Book);
  }

  async fetch(request: Request): Promise<Response> {
    return await this.app.run(request);
  }
}

export default {
  async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
    const app = clo.cloesce(env);
    app.register(Author, Book);
    return await app.run(request);
  },
};
