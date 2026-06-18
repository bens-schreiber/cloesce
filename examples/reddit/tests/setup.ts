import { runInDurableObject } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { beforeAll } from "vitest";
import * as clo from "../.cloesce/backend.js";
import { AuthUser } from "../src/api/auth.js";

declare global {
  namespace Cloudflare {
    interface Env extends clo.CfEnv {}
  }
}

beforeAll(() => clo.cloesce(env).forceLoad());

export function inUser(
  username: string | null,
  as: string | null,
  fn: (env: any) => any,
): Promise<any> {
  const cloEnv = clo.upgradeEnv(env);
  const stub = cloEnv.UserDo.stub(username ?? "");
  return runInDurableObject(stub, (ctx) => fn({ ...cloEnv, ctx, AuthUser: new AuthUser(as) }));
}

export function inSub(subId: string, as: string | null, fn: (env: any) => any): Promise<any> {
  const cloEnv = clo.upgradeEnv(env);
  const stub = cloEnv.SubRedditDo.stub(subId);
  return runInDurableObject(stub, (ctx) => fn({ ...cloEnv, ctx, AuthUser: new AuthUser(as) }));
}

export { env };
