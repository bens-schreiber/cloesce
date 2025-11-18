// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial } from "cloesce/client";
export class InjectedThing {
  value: string;

  static fromJson(data: any): InjectedThing {
    const res = Object.assign(new InjectedThing(), data);
    return res;
  }
}


export class Model {
  id: number;

  static async blockedMethod(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/Model/blockedMethod`);
    const res = await fetchImpl(baseUrl, { method: "GET" });
    return await HttpResult.fromResponse<void>(res);
  }
  static async getInjectedThing(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<InjectedThing>> {
    const baseUrl = new URL(`http://localhost:5002/api/Model/getInjectedThing`);
    const res = await fetchImpl(baseUrl, { method: "GET" });
    return await HttpResult.fromResponse<InjectedThing>(res, InjectedThing, false);
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
    return await HttpResult.fromResponse<Model>(res, Model, false);
  }

  static fromJson(data: any): Model {
    const res = Object.assign(new Model(), data);
    return res;
  }
}
