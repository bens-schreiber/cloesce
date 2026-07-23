import { CfEnv } from "@cloesce/backend.js";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { beforeAll, inject } from "vitest";
import * as api from "../src/api/main.js";
import * as clo from "@cloesce/backend.js";

declare module "vitest" {
  interface ProvidedContext {
    migrations: D1Migration[];
  }
}

declare global {
  namespace Cloudflare {
    interface Env extends CfEnv {}
  }
}

export function app(username?: string) {
  const builder = api.app(env);
  if (!username) {
    return builder;
  }

  return builder.register(clo.AuthUser, { username });
}

beforeAll(async () => {
  // DO migrations are applied by `createApp(..., [migration])` the first time a shard is
  // touched, but the D1 ones backing SubRedditDb have to be applied by the test pool.
  await applyD1Migrations(env.SubRedditDb, inject("migrations"));
  await app().forceLoad();
});

export { env };
