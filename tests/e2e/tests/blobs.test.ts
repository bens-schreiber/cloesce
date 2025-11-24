import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, stopWrangler, withRes } from "../src/setup";
import { BlobHaver } from "../../fixtures/regression/blobs/client";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("../fixtures/regression/blobs");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("CRUD Blob", () => {
  let blobHaver;
  it("POST Blob", async () => {
    const res = await BlobHaver.save({
      blob1: new Uint8Array([1, 2, 3, 4]),
      blob2: new Uint8Array([5, 6, 7, 8]),
    });
  });
});
