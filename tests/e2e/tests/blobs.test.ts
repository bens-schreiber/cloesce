import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { startWrangler, stopWrangler, withRes } from "../src/setup";
import {
  BlobHaver,
  BlobService,
} from "../../fixtures/regression/blobs/client.js";

beforeAll(async () => {
  // NOTE: e2e is called from proj root
  await startWrangler("../fixtures/regression/blobs");
}, 30_000);

afterAll(async () => {
  await stopWrangler();
});

describe("BlobService", () => {
  it("Receives, modifies and returns Uint8Array", async () => {
    const res = await BlobService.incrementBlob(new Uint8Array([1, 2, 3, 4]));
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data).toStrictEqual(new Uint8Array([2, 3, 4, 5]));
  });
});

describe("BlobHaver", () => {
  it("POST Stream", async () => {
    const res = await BlobHaver.inputStream(new Uint8Array([1, 2, 3, 4, 5]));
    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
  });

  let blobHaver: BlobHaver;
  it("POST Blob", async () => {
    const res = await BlobHaver.save({
      blob1: new Uint8Array([1, 2, 3, 4]),
      blob2: new Uint8Array([5, 6, 7, 8]),
    });

    expect(res.ok, withRes("POST should be OK", res)).toBe(true);
    expect(res.data).toStrictEqual(
      Object.assign(new BlobHaver(), {
        id: 1,
        blob1: new Uint8Array([1, 2, 3, 4]),
        blob2: new Uint8Array([5, 6, 7, 8]),
      }),
    );
    blobHaver = res.data;
  });

  it("GET Blob", async () => {
    const res = await blobHaver.getBlob1();
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toStrictEqual(new Uint8Array([1, 2, 3, 4]));
  });

  it("LIST Blobs", async () => {
    const res = await BlobHaver.list();
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);
    expect(res.data).toStrictEqual([blobHaver]);
  });

  it("GET Stream", async () => {
    const res = await blobHaver.yieldStream();
    expect(res.ok, withRes("GET should be OK", res)).toBe(true);

    const got: number[] = Array.from(res.data);
    const expected = [1, 2, 3, 4];
    expect(
      expected.length === got.length && expected.every((v, i) => v === got[i]),
    );
  });
});
