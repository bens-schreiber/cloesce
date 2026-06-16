import { env } from "cloudflare:workers";
import { beforeAll } from "vitest";
import * as clo from "../.cloesce/backend.js";

declare global {
  namespace Cloudflare {
    interface Env extends clo.CfEnv {}
  }
}

beforeAll(() => clo.cloesce(env).forceLoad());

export function upgraded() {
  return clo.upgradeEnv(env);
}

export { env };
