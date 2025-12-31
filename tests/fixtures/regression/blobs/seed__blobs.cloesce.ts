import {
  D1,
  PrimaryKey,
  WranglerEnv,
  CRUD,
  GET,
  Service,
  POST,
  Integer
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";
type Integer = number & { __kind: "Integer" };

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

    return blob.map((b) => b + 1);
  }
}

@D1
@CRUD(["SAVE", "GET", "LIST"])
export class BlobHaver {
  @PrimaryKey
  id: Integer;

  blob1: Uint8Array;
  blob2: Uint8Array;

  @GET
  getBlob1(): Uint8Array {
    return this.blob1;
  }

  @POST
  static async inputStream(stream: ReadableStream) {
    if (!(stream instanceof ReadableStream)) {
      throw new Error("Did not receive a stream");
    }

    const value: Uint8Array = (await stream.getReader().read()).value;
    if (!(value instanceof Uint8Array)) {
      throw new Error("Did not receive a uint8array");
    }

    const expected = [1, 2, 3, 4, 5];
    const got = Array.from(value);
    if (
      expected.length !== got.length ||
      !expected.every((v, i) => v === got[i])
    ) {
      throw new Error(
        `Arrays did not match, got: ${got}; expected: ${expected} `
      );
    }
  }

  @GET
  yieldStream(): ReadableStream {
    return new ReadableStream({
      start(controller) {
        controller.enqueue(this.blob1);
        controller.close();
      },
    });
  }
}
