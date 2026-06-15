import { runInDurableObject } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { beforeAll } from "vitest";
import * as clo from "../.cloesce/backend.js";
import { AuthUser } from "../src/api/auth.js";

declare global {
    namespace Cloudflare {
        interface Env extends clo.Env { }
    }
}

beforeAll(() => clo.cloesce(env).forceLoad());

const stub = (ns: DurableObjectNamespace, name: string) => ns.get(ns.idFromName(name));

export const inUser = (username: string | null, as: string | null, fn: (env: any) => any): Promise<any> =>
    runInDurableObject(stub(env.UserDo, clo.UserDo.Shard.template(username ?? as ?? "")), (ctx) =>
        fn({ ...env, ctx, AuthUser: new AuthUser(as) }),
    );

export const inSub = (subId: string, as: string | null, fn: (env: any) => any): Promise<any> =>
    runInDurableObject(stub(env.SubRedditDo, clo.SubRedditDo.Shard.template(subId)), (ctx) =>
        fn({ ...env, ctx, AuthUser: new AuthUser(as) }),
    );

export { env };
