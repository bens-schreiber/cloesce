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

  blob1: Blob;
  blob2: Blob;

  @GET
  getBlob1(): Blob {
    return this.blob1;
  }
}
