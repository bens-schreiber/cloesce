import {
  Model,
  Post,
  WranglerEnv,
  Inject,
  Integer,
  HttpResult,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@Model()
export class NullabilityChecks {
  id: Integer;

  notNullableString: string;
  nullableString: string | null;

  @Post()
  primitiveTypes(a: Integer | null, b: string | null): boolean | null {
    return null;
  }

  @Post()
  modelTypes(a: NullabilityChecks | null): NullabilityChecks | null {
    return null;
  }

  @Post()
  injectableTypes(@Inject env: Env | null) { }

  @Post()
  arrayTypes(
    a: Integer[] | null,
    b: NullabilityChecks[] | null,
  ): string[] | null {
    return null;
  }

  @Post()
  httpResultTypes(): HttpResult<NullabilityChecks[] | null> | null {
    return null;
  }
}
