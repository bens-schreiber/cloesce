import { DurableObject } from "cloudflare:workers";
import { createApp, Worker, LibraryDoHost, Author, Book, type CfEnv } from "./backend.js";
import libraryDoInitial from "./migrations/LibraryDo/Initial.js";

export class LibraryDo extends DurableObject<CfEnv> {
  private base = createApp(this, LibraryDoHost, [libraryDoInitial]).register(Book, {});
  async fetch(request: Request): Promise<Response> {
    return this.base.run(request);
  }
}

export default {
  async fetch(request: Request, env: CfEnv): Promise<Response> {
    return createApp(env, Worker).register(Author, {}).register(Book, {}).run(request);
  },
};
