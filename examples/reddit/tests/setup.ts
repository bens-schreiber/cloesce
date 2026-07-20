import { applyD1Migrations, runInDurableObject, type D1Migration } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { beforeAll, inject } from "vitest";
import * as clo from "../.cloesce/backend.js";
import { AuthUser } from "../src/api/auth.js";

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
  // DO migrations are applied by `this.cloesce(env, [...])`, but the D1 ones
  // backing SubRedditDb have to be applied by the test pool.
  await applyD1Migrations(env.SubRedditDb, inject("migrations"));
  await clo.cloesce(env).forceLoad();
});

// The router only ever injects the bindings a method declares via `[inject ...]`,
// so tests name them explicitly too. Handing a method the whole env instead would
// hide a missing `[inject]` here and fail in a real Worker.
type Binding = keyof clo.Env;

function envWith(as: string | null, bindings: Binding[], ctx?: unknown): any {
  const cloEnv = clo.upgradeEnv(env) as any;
  const scoped: any = { AuthUser: new AuthUser(as) };
  for (const b of bindings) scoped[b] = cloEnv[b];
  if (ctx !== undefined) scoped.ctx = ctx;
  return scoped;
}

export function inUser(
  username: string | null,
  as: string | null,
  fn: (env: any) => any,
  bindings: Binding[] = [],
): Promise<any> {
  const stub = clo.upgradeEnv(env).UserDo.stub(username ?? "");
  return runInDurableObject(stub, (ctx) => fn(envWith(as, bindings, ctx)));
}

export function inPost(
  doId: string,
  as: string | null,
  fn: (env: any) => any,
  bindings: Binding[] = [],
): Promise<any> {
  const stub = clo.upgradeEnv(env).PostDo.stub(doId);
  return runInDurableObject(stub, (ctx) => fn(envWith(as, bindings, ctx)));
}

// `SubReddit.create`, `Post.create` and `SubReddit.feed` run on the Worker
// against D1, so they get an env with no DO context at all.
export function onWorker(
  as: string | null,
  fn: (env: any) => any,
  bindings: Binding[] = [],
): Promise<any> {
  return fn(envWith(as, bindings));
}

export { env };
