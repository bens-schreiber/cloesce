// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";


export class Weather {
  id: number;
  date: Date;
  isRaining: boolean;

  static async get(
        id: number,
        __datasource: "none" = "none",
  ): Promise<HttpResult<Weather>> {
    const baseUrl = new URL(`http://localhost:5002/api/Weather/get`);
    baseUrl.searchParams.append('id', String(id));
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetch(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new Weather(), raw.data);
    return raw;
  }
  static async save(
        model: DeepPartial<Weather>,
        __datasource: "none" = "none",
  ): Promise<HttpResult<Weather>> {
    const baseUrl = new URL(`http://localhost:5002/api/Weather/save`);
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            model, 
            __datasource
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new Weather(), raw.data);
    return raw;
  }
}
