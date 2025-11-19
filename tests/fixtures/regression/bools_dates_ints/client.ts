// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial } from "cloesce/client";


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
    return await HttpResult.fromResponse<Weather>(res, Weather, false);
  }
  static async save(
    model: DeepPartial<Weather>,
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Weather>> {
    const baseUrl = new URL(`http://localhost:5002/api/Weather/save`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            model, 
            __datasource
      })
    });
    return await HttpResult.fromResponse<Weather>(res, Weather, false);
  }

  static fromJson(data: any): Weather {
    const res = Object.assign(new Weather(), data);
    return res;
  }
}
