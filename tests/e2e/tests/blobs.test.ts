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
    console.log(res.status);
    console.log(res.message);
  });
});

describe("CRUD Blob", () => {
  // let blobHaver;
  // it("POST Blob", async () => {
  //   // const baseUrl = new URL(`http://localhost:5002/api/BlobHaver/save`);
  //   // const res = await fetch(baseUrl, {
  //   //   method: "POST",
  //   //   body: JSON.stringify({
  //   //     model: {},
  //   //     __dataSource: "none",
  //   //   }),
  //   // });
  //   // console.log(JSON.stringify(res));
  //   // return await HttpResult.fromResponse<BlobHaver>(
  //   //   res,
  //   //   MediaType.FormData,
  //   //   BlobHaver,
  //   //   false
  //   // );
  //   // const res = await BlobHaver.save({
  //   //   // blob1: new Uint8Array([1, 2, 3, 4]),
  //   //   // blob2: new Uint8Array([5, 6, 7, 8]),
  //   // });
  // });
});
