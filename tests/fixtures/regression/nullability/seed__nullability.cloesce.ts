import { D1, POST, PrimaryKey, WranglerEnv, Inject } from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";
type HttpResult<T = unknown> = {};
type Integer = number & { __kind: "Integer" };

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
export class NullabilityChecks {
  @PrimaryKey
  id: Integer;

  notNullableString: string;
  nullableString: string | null;

  @POST
  primitiveTypes(a: Integer | null, b: string | null): boolean | null {
    return null;
  }

  @POST
  modelTypes(a: NullabilityChecks | null): NullabilityChecks | null {
    return null;
  }

  @POST
  injectableTypes(@Inject env: Env | null) {}

  @POST
  arrayTypes(
    a: Integer[] | null,
    b: NullabilityChecks[] | null
  ): string[] | null {
    return null;
  }

  @POST
  httpResultTypes(
    a: HttpResult<Integer | null> | null
  ): HttpResult<NullabilityChecks[] | null> | null {
    return null;
  }
}
