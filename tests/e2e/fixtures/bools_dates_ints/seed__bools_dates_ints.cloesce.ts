import { Model, WranglerEnv, Integer } from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@Model(["SAVE", "GET"])
export class Weather {
  id: Integer;

  date: Date;
  isRaining: boolean;
}
