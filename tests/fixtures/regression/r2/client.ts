// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8, KValue, R2Object } from "cloesce/client";



export class D1BackedModel {
  id: number;
  someColumn: number;
  someOtherColumn: string;
  keyParam: string;
  r2Data: R2Object;

  static async get(
    id: number,
    keyParam: string,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<D1BackedModel>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/get`
    );

    baseUrl.searchParams.append("id", String(id));
    baseUrl.searchParams.append("keyParam", String(keyParam));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      D1BackedModel,
      false
    );
  }
  static async list(
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<D1BackedModel[]>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/list`
    );

    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      D1BackedModel,
      true
    );
  }
  static async save(
    model: DeepPartial<D1BackedModel>,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<D1BackedModel>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/save`
    );
    const payload: any = {};

    payload["model"] = model;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      D1BackedModel,
      false
    );
  }
  async uploadData(
    stream: Uint8Array,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const id = [
      encodeURIComponent(String(this.id)),
      encodeURIComponent(String(this.keyParam)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/${id}/uploadData`
    );
    const payload: any = {};

    payload["stream"] = stream;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "PUT",
      duplex: "half",
      headers: { "Content-Type": "application/octet-stream" },
      body: requestBody(MediaType.Octet, payload),
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
export class PureR2Model {
  id: string;
  data: R2Object;
  otherData: R2Object;
  allData: R2Object[];

  static async get(
    id: string,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<PureR2Model>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/PureR2Model/get`
    );

    baseUrl.searchParams.append("id", String(id));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      PureR2Model,
      false
    );
  }
  async uploadData(
    stream: Uint8Array,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/PureR2Model/${id}/uploadData`
    );
    const payload: any = {};

    payload["stream"] = stream;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "PUT",
      duplex: "half",
      headers: { "Content-Type": "application/octet-stream" },
      body: requestBody(MediaType.Octet, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      undefined,
      false
    );
  }
  async uploadOtherData(
    stream: Uint8Array,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/PureR2Model/${id}/uploadOtherData`
    );
    const payload: any = {};

    payload["stream"] = stream;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "PUT",
      duplex: "half",
      headers: { "Content-Type": "application/octet-stream" },
      body: requestBody(MediaType.Octet, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): PureR2Model {
    const res = Object.assign(new PureR2Model(), data);
    return res;
  }
}
