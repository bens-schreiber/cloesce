// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8 } from "cloesce/client";

export class BlobService {
  static async incrementBlob(
    blob: Uint8Array,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Uint8Array>> {
    const baseUrl = new URL("http://localhost:5002/api/BlobService/incrementBlob");
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, {
            blob
      })
    });

    return await HttpResult.fromResponse<Uint8Array>(
      res, 
      MediaType.Json,
      Uint8Array,
      false
    );
    }
}

export class BlobHaver {
  id: number;
  blob1: Uint8Array;
  blob2: Uint8Array;

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
      MediaType.Json,
      BlobHaver,
      false
    );
  }
  async getBlob1(
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Uint8Array>> {
    const baseUrl = new URL(`http://localhost:5002/api/BlobHaver/${this.id}/getBlob1`);
    baseUrl.searchParams.append('__dataSource', String(__dataSource));
    const res = await fetchImpl(baseUrl, { method: "GET" });

    return await HttpResult.fromResponse<Uint8Array>(
      res, 
      MediaType.Json,
      Uint8Array,
      false
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
      MediaType.Json,
      BlobHaver,
      true
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
      body: requestBody(MediaType.Json, {
            model, 
            __datasource
      })
    });

    return await HttpResult.fromResponse<BlobHaver>(
      res, 
      MediaType.Json,
      BlobHaver,
      false
    );
  }

  static fromJson(data: any): BlobHaver {
    const res = Object.assign(new BlobHaver(), data);

    res.blob1 = b64ToU8(res.blob1);
    res.blob2 = b64ToU8(res.blob2);

    return res;
  }
}
