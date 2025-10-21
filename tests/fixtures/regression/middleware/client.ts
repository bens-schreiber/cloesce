import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";

export class Model {
  id: number;

  static async post(obj: DeepPartial<Model>, dataSource: null = null): Promise<HttpResult<Model>> {
    const baseUrl = new URL(`http://localhost:5002/api/Model/POST`);
    baseUrl.searchParams.append("dataSource", String(dataSource));

    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        obj
      })
    });

    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }

    raw.data = Object.assign(new Model(), raw.data);
    return raw;
  }
}
