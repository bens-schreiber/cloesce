import { HttpResult } from "cloesce";
import * as clo from "./backend.js";

const BlobService = clo.BlobService.impl({
  incrementBlob(blob: Uint8Array) {
    if (!(blob instanceof Uint8Array)) {
      throw new Error(`Received blob was not an instance of uint8array: ${JSON.stringify(blob)}`);
    }

    // Add 1 to each byte in the blob
    return HttpResult.ok(
      200,
      blob.map((b) => b + 1),
    );
  },
});

const BlobHaver = clo.BlobHaver.impl({
  // Returns a stream of its own blob1 column
  yieldStream(self: clo.BlobHaver.Self): HttpResult<clo.CfReadableStream> {
    const blob1 = self.blob1;
    return HttpResult.ok(
      200,
      new ReadableStream({
        start(controller) {
          controller.enqueue(blob1);
          controller.close();
        },
      }) as any,
    );
  },

  getBlob1(self: clo.BlobHaver.Self) {
    return HttpResult.ok(200, self.blob1);
  },

  // Accepts some stream and validates that it sent [1, 2, 3, 4, 5]
  async inputStream(stream: clo.CfReadableStream) {
    if (!(stream instanceof ReadableStream)) {
      throw new Error("Did not receive a stream");
    }

    const value: Uint8Array = (await stream.getReader().read()).value;
    if (!(value instanceof Uint8Array)) {
      throw new Error("Did not receive a uint8array");
    }

    const expected = [1, 2, 3, 4, 5];
    const got = Array.from(value);
    if (expected.length !== got.length || !expected.every((v, i) => v === got[i])) {
      throw new Error(`Arrays did not match, got: ${got}; expected: ${expected} `);
    }

    return HttpResult.ok(200);
  },
});

export default {
  async fetch(request: Request, env: clo.CfEnv): Promise<Response> {
    const app = clo.cloesce(env);
    app.register(BlobService, BlobHaver);

    return app.run(request);
  },
};
