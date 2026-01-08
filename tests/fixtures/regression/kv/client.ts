// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8, KValue, R2Object } from "cloesce/client";



export class D1BackedModel {
  id: number;
  someColumn: number;
  someOtherColumn: string;
  keyParam: string;
  kvData: KValue<unknown>;

  static async post(
    model: DeepPartial<D1BackedModel>,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/post`
    );
    const payload: any = {};

    payload["model"] = model;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): D1BackedModel {
    const res = Object.assign(new D1BackedModel(), data);
    return res;
  }
}
export class PureKVModel {
  id: string;
  data: KValue<unknown>;
  otherData: KValue<string>;

  static async post(
    id: string,
    data: unknown,
    otherData: string,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/PureKVModel/post`
    );
    const payload: any = {};

    payload["id"] = id;
    payload["data"] = data;
    payload["otherData"] = otherData;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): PureKVModel {
    const res = Object.assign(new PureKVModel(), data);
    return res;
  }
}
