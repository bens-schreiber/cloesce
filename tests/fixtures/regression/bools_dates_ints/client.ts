// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType } from "cloesce/client";


export class Weather {
  id: number;
  date: Date;
  isRaining: boolean;

  static async get(
    id: number,
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Weather>> {
    const baseUrl = new URL(`http://localhost:5002/api/Weather/get`);
    baseUrl.searchParams.append('id', String(id));
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });

    return await HttpResult.fromResponse<Weather>(
      res, 
      MediaType.Json,
      Weather, false
    );
  }
  static async save(
    model: DeepPartial<Weather>,
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Weather>> {
    const baseUrl = new URL(`http://localhost:5002/api/Weather/save`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      body: JSON.stringify({
            model, 
            __datasource
      })
    });

    return await HttpResult.fromResponse<Weather>(
      res, 
      MediaType.Json,
      Weather, false
    );
  }

  static fromJson(data: any, blobs: Uint8Array[]): Weather {
    const res = Object.assign(new Weather(), data);


    return res;
  }
}
