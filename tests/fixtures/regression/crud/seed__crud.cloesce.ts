import {
  D1,
  PrimaryKey,
  WranglerEnv,
  CRUD,
  CrudKind,
  POST,
} from "cloesce/backend";

import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@CRUD(["POST", "PATCH", "GET", "LIST"])
@D1
export class CrudHaver {
  @PrimaryKey
  id: number;

  @POST
  async notCrud(): Promise<void> {}
}
