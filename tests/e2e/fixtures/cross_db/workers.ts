import { DurableObject } from "cloudflare:workers";
import { createApp, Author, Book, type CfEnv } from "./backend.js";
import libraryDoInitial from "./migrations/LibraryDo/Initial.js";

function app() {
  return createApp().register(Author, {}).register(Book, {});
}

export class LibraryDo extends DurableObject<CfEnv> {
  private base = app().durable(this, [libraryDoInitial]);
  async fetch(request: Request): Promise<Response> {
    return this.base.run(request);
  }
}

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return app().worker(env).run(request);
  },
};
