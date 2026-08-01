import { Auth, FullEnv, User, UserDto, ProfileDto } from "@cloesce/backend.js";
import { HttpResult } from "cloesce";

declare module "@cloesce/backend.js" {
  interface Auth {
    /**
     * The logged-in `User`, or `null` for an anonymous request.
     */
    user: User | null;
  }
}

function mockToken(user: User) {
  return `mock.jwt.${user.id}`;
}

export async function authFromRequest(env: FullEnv, request: Request): Promise<Auth> {
  const id = Number(request.headers.get("Authorization")?.match(/mock\.jwt\.(\d+)$/)?.[1]);
  return { user: id ? ((await env.db.user.get(id)).data ?? null) : null };
}

export function requireAuth(env: { Auth: Auth }): User | HttpResult<never> {
  return env.Auth.user ?? HttpResult.fail(401, "You must be logged in.");
}

export function userDto(user: User): UserDto {
  return {
    email: user.email,
    username: user.username,
    bio: user.bio,
    image: user.image,
    token: mockToken(user),
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
