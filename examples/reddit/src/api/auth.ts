import * as clo from "@cloesce/backend.js";
import { HttpResult } from "cloesce";

export class AuthUser extends clo.AuthUser {
    constructor(public readonly username: string | null) {
        super();
    }

    static async fromRequest(sessions: clo.Env.Sessions, request: Request): Promise<AuthUser> {
        const token = request.headers.get("Authorization")?.match(/^Bearer\s+(.+)$/i)?.[1];
        const username = token ? await sessions.session.get(token) : null;
        return new AuthUser((username as string | null) ?? null);
    }

    static newToken(): string {
        return crypto.randomUUID();
    }
}

// TODO: Reintroduce Cloesce middleware so impls don't have to opt in by hand.
export function auth<T>(
    env: { AuthUser: clo.AuthUser },
    fn: (username: string) => Promise<T>,
): Promise<T | HttpResult<never>> {
    const { username } = env.AuthUser as AuthUser;
    return username === null ? Promise.resolve(HttpResult.fail(401, "You must be logged in.")) : fn(username);
}
