import { env } from "cloudflare:workers";
import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { beforeAll, inject } from "vitest";
import * as clo from "../.cloesce/backend.js";

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

beforeAll(async () => {
  const migrations = inject("migrations");
  await applyD1Migrations(env.db, migrations);
  await clo.cloesce(env).forceLoad();
});

export function upgraded() {
  return clo.upgradeEnv(env);
}

export { env };
