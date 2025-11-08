// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";

export class InjectedThing {
  value: string;
}

export class Model {
  id: number;

  static async blockedMethod(
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/Model/blockedMethod`);
    const res = await fetch(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
  static async getInjectedThing(
  ): Promise<HttpResult<InjectedThing>> {
    const baseUrl = new URL(`http://localhost:5002/api/Model/getInjectedThing`);
    const res = await fetch(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new InjectedThing(), raw.data);
    return raw;
  }
  static async save(
        model: DeepPartial<Model>,
        __datasource: "none" = "none",
  ): Promise<HttpResult<Model>> {
    const baseUrl = new URL(`http://localhost:5002/api/Model/save`);
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
    raw.data = Object.assign(new Model(), raw.data);
    return raw;
  }
}
