import { Auth, Env, User, UserDto, ProfileDto } from "@cloesce/backend.js";
import { HttpResult } from "cloesce";

declare module "@cloesce/backend.js" {
  interface Auth {
    /**
     * The logged-in `User`, or `null` for an anonymous request.
     */
    user: User | null;

    /**
     * The session token that authenticated this request, if any.
     * `userDto` echoes this back to the client as the `Authorization` value
     * to keep using; `register`/`login` mint a fresh one instead, since
     * there is no incoming session to reuse.
     */
    token: string | null;
  }
}

export async function issueToken(session: Env.Session, userId: number): Promise<string> {
  const token = crypto.randomUUID();
  await session.session.put(token, userId);
  return token;
}

export async function authFromRequest(
  db: Env.Db,
  session: Env.Session,
  request: Request,
): Promise<Auth> {
  const token = request.headers.get("Authorization")?.replace(/^Token\s+/, "") ?? null;
  const id = token ? await session.session.get(token) : null;
  const user = id ? ((await db.User.get(id)).data ?? null) : null;

  return { user, token: user ? token : null };
}

export function requireAuth(env: { Auth: Auth }): User | HttpResult<never> {
  return env.Auth.user ?? HttpResult.fail(401, "You must be logged in.");
}

export function userDto(user: User, token: string): UserDto {
  return {
    email: user.email,
    username: user.username,
    bio: user.bio,
    image: user.image,
    token,
  };
}

export function profileDto(user: User, following: boolean): ProfileDto {
  return {
    username: user.username,
    bio: user.bio,
    image: user.image,
    following,
  };
}
