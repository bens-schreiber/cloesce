import * as clo from "@cloesce/backend.js";
import { HttpResult } from "cloesce";

declare module "@cloesce/backend.js" {
  interface AuthUser {
    username: string | null;
  }
}

export async function authFromRequest(
  sessions: clo.Env.Sessions,
  request: Request,
): Promise<clo.AuthUser> {
  const token = request.headers.get("Authorization")?.match(/^Bearer\s+(.+)$/i)?.[1];
  const username = token ? await sessions.session.get(token) : null;
  return { username: (username as string | null) ?? null };
}

export function newToken(): string {
  return crypto.randomUUID();
}

export function auth(env: { AuthUser?: clo.AuthUser }): string | HttpResult<never> {
  return env.AuthUser?.username ?? HttpResult.fail(401, "You must be logged in.");
}
