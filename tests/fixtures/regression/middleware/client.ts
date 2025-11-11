// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";
export class InjectedThing {
  value: string;
}

export class Model {
  id: number;

  static async blockedMethod(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/Model/blockedMethod`);
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let httpResult = HttpResult<void>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    return httpResult;
  }
  static async getInjectedThing(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<InjectedThing>> {
    const baseUrl = new URL(`http://localhost:5002/api/Model/getInjectedThing`);
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let httpResult = HttpResult<InjectedThing>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new InjectedThing(), httpResult.data);
    return httpResult;
  }
  static async save(
        model: DeepPartial<Model>,
        __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Model>> {
    const baseUrl = new URL(`http://localhost:5002/api/Model/save`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            model, 
            __datasource
      })
    });
    let httpResult = HttpResult<Model>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new Model(), httpResult.data);
    return httpResult;
  }
}
