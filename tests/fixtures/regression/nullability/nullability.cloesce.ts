import { D1, POST, PrimaryKey, WranglerEnv, Inject } from "cloesce";
type HttpResult<T = unknown> = {};
type D1Database = {};

@WranglerEnv
class Env {
  db: D1Database;
}

@D1
export class NullabilityChecks {
  @PrimaryKey
  id: number;

  notNullableString: string;
  nullableString: string | null;

  @POST
  async primitiveTypes(
    a: number | null,
    b: string | null
  ): Promise<boolean | null> {
    return null;
  }

  @POST
  async modelTypes(
    a: NullabilityChecks | null
  ): Promise<NullabilityChecks | null> {
    return null;
  }

  @POST
  async injectableTypes(@Inject env: Env | null) {}

  @POST
  async arrayTypes(
    a: number[] | null,
    b: NullabilityChecks[] | null
  ): Promise<string[] | null> {
    return null;
  }

  @POST
  async httpResultTypes(
    a: HttpResult<number | null> | null
  ): Promise<HttpResult<NullabilityChecks[] | null> | null> {
    return null;
  }
}
