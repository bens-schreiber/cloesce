// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType } from "cloesce/client";


export class BlobHaver {
  id: number;
  blob1: Blob;
  blob2: Blob;

  static async get(
    id: number,
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<BlobHaver>> {
    const baseUrl = new URL(`http://localhost:5002/api/BlobHaver/get`);
    baseUrl.searchParams.append('id', String(id));
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });

    return await HttpResult.fromResponse<BlobHaver>(
      res, 
      MediaType.FormData,
      BlobHaver, false
    );
  }
  async getBlob1(
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Blob>> {
    const baseUrl = new URL(`http://localhost:5002/api/BlobHaver/${this.id}/getBlob1`);
    baseUrl.searchParams.append('__dataSource', String(__dataSource));
    const res = await fetchImpl(baseUrl, { method: "GET" });

    return await HttpResult.fromResponse<Blob>(
      res, 
      MediaType.Octet,
    );
  }
  static async list(
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<BlobHaver[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/BlobHaver/list`);
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });

    return await HttpResult.fromResponse<BlobHaver[]>(
      res, 
      MediaType.FormData,
    );
  }
  static async save(
    model: DeepPartial<BlobHaver>,
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<BlobHaver>> {
    const baseUrl = new URL(`http://localhost:5002/api/BlobHaver/save`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            model, 
            __datasource
      })
    });

    return await HttpResult.fromResponse<BlobHaver>(
      res, 
      MediaType.FormData,
      BlobHaver, false
    );
  }

  static fromJson(data: any): BlobHaver {
    const res = Object.assign(new BlobHaver(), data);

    res.blob1 = blobs[res.blob1.__blobIndex];
    res.blob2 = blobs[res.blob2.__blobIndex];

    return res;
  }
}
