import { D1, PrimaryKey, WranglerEnv, CRUD, GET } from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@D1
@CRUD(["SAVE", "GET", "LIST"])
export class BlobHaver {
  @PrimaryKey
  id: number;

  blob1: Uint8Array;
  blob2: Uint8Array;

  @GET
  getBlob1(): Uint8Array {
    return this.blob1;
  }
}
