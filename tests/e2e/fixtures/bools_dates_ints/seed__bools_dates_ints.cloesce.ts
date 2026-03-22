import { Model, WranglerEnv, Integer, Crud } from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@Crud("SAVE", "GET")
@Model("db")
export class Weather {
  id: Integer;

  date: Date;
  isRaining: boolean;
}
