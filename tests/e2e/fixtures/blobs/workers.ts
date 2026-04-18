import { HttpResult } from "cloesce";
import * as Cloesce from "./backend.js";

const BlobService = Cloesce.BlobService.impl({
    incrementBlob(blob: Uint8Array) {
        if (!(blob instanceof Uint8Array)) {
            throw new Error(
                `Received blob was not an instance of uint8array: ${JSON.stringify(
                    blob,
                )}`,
            );
        }

        return HttpResult.ok(200, blob.map((b) => b + 1));
    },
});

const BlobHaver = Cloesce.BlobHaver.impl({
    yieldStream(self: Cloesce.BlobHaver.Self): HttpResult<Cloesce.CfReadableStream> {
        const blob1 = self.blob1;
        return HttpResult.ok(200, new ReadableStream({
            start(controller) {
                controller.enqueue(blob1);
                controller.close();
            },
        }) as any);
    },

    getBlob1(self: Cloesce.BlobHaver.Self) {
        return HttpResult.ok(200, self.blob1);
    },

    async inputStream(stream: Cloesce.CfReadableStream) {
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

        return HttpResult.ok(200);
    },
});

export default {
    async fetch(request: Request, env: Cloesce.Env): Promise<Response> {
        const app = await Cloesce.cloesce();
        app.register(BlobService)
            .register(BlobHaver);

        return app.run(request, env);
    }
}