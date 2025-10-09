import { HttpResult, instantiateObjectArray } from "cloesce";

export class House {
  id: number;
  name: string;

  static async get(
        id: number,
    dataSource: null = null
  ): Promise<HttpResult<House>> {
    const baseUrl = new URL(`http://localhost:5002/api/House/get`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    baseUrl.searchParams.append('id', String(id));
    const res = await fetch(baseUrl, { method: "GET" });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    raw.data = Object.assign(new House(), raw.data);
    return raw;
  }
}
