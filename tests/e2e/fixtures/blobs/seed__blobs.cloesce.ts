import {
  Model,
  WranglerEnv,
  Get,
  Service,
  Post,
  Integer,
} from "cloesce/backend";
import { D1Database } from "@cloudflare/workers-types";

@WranglerEnv
export class Env {
  db: D1Database;
}

@Service
export class BlobService {
  @Post()
  incrementBlob(blob: Uint8Array): Uint8Array {
    if (!(blob instanceof Uint8Array)) {
      throw new Error(
        `Received blob was not an instance of uint8array: ${JSON.stringify(
          blob,
        )}`,
      );
    }

    return blob.map((b) => b + 1);
  }
}

@Model(["SAVE", "GET", "LIST"])
export class BlobHaver {
  id: Integer;

  blob1: Uint8Array;
  blob2: Uint8Array;

  @Get()
  getBlob1(): Uint8Array {
    return this.blob1;
  }

  @Post()
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
        `Arrays did not match, got: ${got}; expected: ${expected} `,
      );
    }
  }

  @Get()
  yieldStream(): ReadableStream {
    const blob1 = this.blob1;
    return new ReadableStream({
      start(controller) {
        controller.enqueue(blob1);
        controller.close();
      },
    });
  }
}
