// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8 } from "cloesce/client";

export class BlobService {
  static async incrementBlob(
    blob: Uint8Array,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Uint8Array>> {
    const baseUrl = new URL("http://localhost:5002/api/BlobService/incrementBlob");
    const payload: any = {};

      payload["blob"] = blob;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
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

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
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

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      Uint8Array,
      false
    );
  }
  static async inputStream(
    stream: ReadableStream<Uint8Array>,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/BlobHaver/inputStream`);
    const payload: any = {};

      payload["stream"] = stream;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/octet-stream" },
      body: requestBody(MediaType.Octet, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }
  static async list(
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<BlobHaver[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/BlobHaver/list`);

      baseUrl.searchParams.append('__datasource', String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
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
    const payload: any = {};

      payload["model"] = model;
      baseUrl.searchParams.append('__datasource', String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      BlobHaver,
      false
    );
  }
  async yieldStream(
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<ReadableStream<Uint8Array>>> {
    const baseUrl = new URL(`http://localhost:5002/api/BlobHaver/${this.id}/yieldStream`);

      baseUrl.searchParams.append('__dataSource', String(__dataSource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Octet,
      ReadableStream<Uint8Array>,
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
