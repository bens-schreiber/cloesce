import { runInDurableObject } from "cloudflare:test";
import { env } from "cloudflare:workers";
import { beforeAll } from "vitest";
import * as clo from "../.cloesce/backend.js";
import { AuthUser } from "../src/api/auth.js";

declare global {
    namespace Cloudflare {
        interface Env extends clo.CfEnv { }
    }
}

beforeAll(() => clo.cloesce(env).forceLoad());

function stub(ns: DurableObjectNamespace, name: string) {
    return ns.get(ns.idFromName(name));
}

function upgraded() {
    return clo.upgradeEnv(env);
}

export function inUser(username: string | null, as: string | null, fn: (env: any) => any): Promise<any> {
    return runInDurableObject(stub(env.UserDo, clo.UserDo.Shard.template(username ?? as ?? "")), (ctx) => fn({ ...upgraded(), ctx, AuthUser: new AuthUser(as) })
    );
}

export function inSub(subId: string, as: string | null, fn: (env: any) => any): Promise<any> {
    return runInDurableObject(stub(env.SubRedditDo, clo.SubRedditDo.Shard.template(subId)), (ctx) => fn({ ...upgraded(), ctx, AuthUser: new AuthUser(as) })
    );
}

export { env };
