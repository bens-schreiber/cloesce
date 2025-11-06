// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";


export class Weather {
  id: number;
  date: Date;
  isRaining: boolean;

  static async get(
        id: number,
        dataSource: "none" = "none",
  ): Promise<HttpResult<Weather>> {
    const baseUrl = new URL(`http://localhost:5002/api/Weather/get`);
    baseUrl.searchParams.append('id', String(id));
    baseUrl.searchParams.append('dataSource', String(dataSource));
    const res = await fetch(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new Weather(), raw.data);
    return raw;
  }
  static async post(
        obj: DeepPartial<Weather>,
        dataSource: "none" = "none",
  ): Promise<HttpResult<Weather>> {
    const baseUrl = new URL(`http://localhost:5002/api/Weather/post`);
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            obj, 
            dataSource
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
