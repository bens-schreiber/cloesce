// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8 } from "cloesce/client";



export class JsonKV {
  key: string;
  value: unknown;
  metadata: unknown;

  async delete(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/JsonKV/${this.}/delete`);
    const payload: any = {};


    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }
  static async get(
    key: string,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<JsonKV>> {
    const baseUrl = new URL(`http://localhost:5002/api/JsonKV/get`);

    baseUrl.searchParams.append('key', String(key));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      JsonKV,
      false
    );
  }
  static async put(
    key: string,
    json: unknown,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/JsonKV/put`);
    const payload: any = {};

    payload["key"] = key;
    payload["json"] = json;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): JsonKV {
    const res = Object.assign(new JsonKV(), data);


    return res;
  }
}
export class TextKV {
  key: string;
  value: string;
  metadata: unknown;

  async delete(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/TextKV/${this.}/delete`);
    const payload: any = {};


    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }
  static async get(
    key: string,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<TextKV>> {
    const baseUrl = new URL(`http://localhost:5002/api/TextKV/get`);

    baseUrl.searchParams.append('key', String(key));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      TextKV,
      false
    );
  }
  static async put(
    key: string,
    value: string,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/TextKV/put`);
    const payload: any = {};

    payload["key"] = key;
    payload["value"] = value;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): TextKV {
    const res = Object.assign(new TextKV(), data);


    return res;
  }
}
