import { D1, POST, PrimaryKey, WranglerEnv, Inject } from "cloesce/backend";
type HttpResult<T = unknown> = {};
type D1Database = {};

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
export class NullabilityChecks {
  @PrimaryKey
  id: number;

  notNullableString: string;
  nullableString: string | null;

  @POST
  primitiveTypes(a: number | null, b: string | null): boolean | null {
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
    a: number[] | null,
    b: NullabilityChecks[] | null
  ): string[] | null {
    return null;
  }

  @POST
  httpResultTypes(
    a: HttpResult<number | null> | null
  ): HttpResult<NullabilityChecks[] | null> | null {
    return null;
  }
}
