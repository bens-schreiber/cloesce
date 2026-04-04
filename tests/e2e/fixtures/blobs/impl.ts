import { HttpResult } from "cloesce";
import * as Cloesce from "./backend.js";
import { ReadableStream } from "@cloudflare/workers-types";

class BlobService extends Cloesce.BlobService.Api {
    init(self: Cloesce.BlobService.Self): void { }

    incrementBlob(blob: Uint8Array): HttpResult<Uint8Array> {
        if (!(blob instanceof Uint8Array)) {
            throw new Error(
                `Received blob was not an instance of uint8array: ${JSON.stringify(
                    blob,
                )}`,
            );
        }

        return HttpResult.ok(200, blob.map((b) => b + 1));
    }
}

class BlobHaver extends Cloesce.BlobHaver.Api {
    yieldStream(self: Cloesce.BlobHaver.Self): HttpResult<ReadableStream<any>> {
        const blob1 = self.blob1;
        return HttpResult.ok(200, new ReadableStream({
            start(controller) {
                controller.enqueue(blob1);
                controller.close();
            },
        }));
    }

    getBlob1(self: Cloesce.BlobHaver.Self): HttpResult<Uint8Array> {
        return HttpResult.ok(200, self.blob1);
    }

    async inputStream(stream: ReadableStream): Promise<HttpResult<void>> {
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
    }
}

export default async function fetch(request: Request, env: Cloesce.Env): Promise<Response> {
    const app = await Cloesce.cloesce();
    app.register(new BlobService())
        .register(new BlobHaver());

    return app.run(request, env);
}