import { Model, PrimaryKey, WranglerEnv, CRUD } from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";
type Integer = number & { __kind: "Integer" };

@WranglerEnv
export class Env {
  db: D1Database;
}

@Model
@CRUD(["SAVE", "GET"])
export class Weather {
  @PrimaryKey
  id: Integer;

  date: Date;
  isRaining: boolean;
}
