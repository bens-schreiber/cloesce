import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { beforeAll, inject } from "vitest";
import * as clo from "../.cloesce/backend.js";
import * as api from "@api/main.js";

declare module "vitest" {
  interface ProvidedContext {
    migrations: D1Migration[];
  }
}

declare global {
  namespace Cloudflare {
    interface Env extends clo.CfEnv {}
  }
}

export const app = api.app(env);

beforeAll(async () => {
  const migrations = inject("migrations");
  await applyD1Migrations(env.db, migrations);
  await app.forceLoad();
});

export { env };
