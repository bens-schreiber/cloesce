import {
  D1,
  PrimaryKey,
  WranglerEnv,
  CRUD,
  GET,
  Service,
  POST,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@Service
export class BlobService {
  @POST
  incrementBlob(blob: Uint8Array): Uint8Array {
    if (!(blob instanceof Uint8Array)) {
      throw new Error(
        `Received blob was not an instance of uint8array: ${JSON.stringify(
          blob
        )}`
      );
    }

    blob.map((b) => b + 1);
    return blob;
  }
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
