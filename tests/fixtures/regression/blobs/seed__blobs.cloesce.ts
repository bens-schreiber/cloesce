import { D1, PrimaryKey, WranglerEnv } from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
export class BlobHaver {
  @PrimaryKey
  id: number;

  blob1: Blob;
  blob2: Blob;
}
