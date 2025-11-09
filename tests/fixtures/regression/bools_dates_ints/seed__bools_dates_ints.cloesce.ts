import { D1, PrimaryKey, WranglerEnv, CRUD } from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";
type Integer = number & { __kind: "Integer" };

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
@CRUD(["SAVE", "GET"])
export class Weather {
  @PrimaryKey
  id: Integer;

  date: Date;
  isRaining: boolean;
}
