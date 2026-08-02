import { applyD1Migrations, type D1Migration } from "cloudflare:test";
import { HttpResult } from "@cloesce/client.js";
import type { CfEnv } from "@cloesce/backend.js";
import { env, exports } from "cloudflare:workers";
import { beforeAll, inject } from "vitest";

declare module "vitest" {
  interface ProvidedContext {
    migrations: D1Migration[];
  }
}

declare global {
  namespace Cloudflare {
    interface Env extends CfEnv {}

    interface GlobalProps {
      mainModule: typeof import("../src/api/main.js");
      durableNamespaces: "FavoriteDo";
    }
  }
}

/**
 * A `fetch` that presents `token` as the caller's credentials.
 */
export function as(token: string | null): typeof fetch {
  return (url, init) =>
    exports.default.fetch(
      new Request(String(url), {
        ...init,
        headers: {
          ...(init?.headers as Record<string, string>),
          ...(token ? { Authorization: `Token ${token}` } : {}),
        },
      }),
    );
}

/** anonymous caller. */
export const anon: typeof fetch = as(null);

export function expectOk<T>(res: HttpResult<T>): T {
  if (!res.ok) {
    throw new Error(`expected an ok result, got ${res.status}: ${res.message}`);
  }
  return res.data as T;
}

beforeAll(async () => {
  await applyD1Migrations(env.Db, inject("migrations"));
});
