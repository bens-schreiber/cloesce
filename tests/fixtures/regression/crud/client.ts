// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";

export class Child {
  id: number;
  parentId: number;
  parent: Parent | undefined;

}
export class CrudHaver {
  id: number;
  name: string;

  static async get(
        id: number,
        __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<CrudHaver>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/get`);
    baseUrl.searchParams.append('id', String(id));
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let httpResult = HttpResult<CrudHaver>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new CrudHaver(), httpResult.data);
    return httpResult;
  }
  static async list(
        __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<CrudHaver[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/list`);
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let httpResult = HttpResult<CrudHaver[]>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = instantiateObjectArray(httpResult.data, CrudHaver);
    return httpResult;
  }
  async notCrud(
        __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/${this.id}/notCrud`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            __dataSource
      })
    });
    let httpResult = HttpResult<void>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    return httpResult;
  }
  static async save(
        model: DeepPartial<CrudHaver>,
        __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<CrudHaver>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/save`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            model, 
            __datasource
      })
    });
    let httpResult = HttpResult<CrudHaver>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new CrudHaver(), httpResult.data);
    return httpResult;
  }
}
export class Parent {
  id: number;
  favoriteChildId: number | null;
  favoriteChild: Child | undefined;
  children: Child[];

  static async get(
        id: number,
        __datasource: "withChildren" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Parent>> {
    const baseUrl = new URL(`http://localhost:5002/api/Parent/get`);
    baseUrl.searchParams.append('id', String(id));
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let httpResult = HttpResult<Parent>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new Parent(), httpResult.data);
    return httpResult;
  }
  static async list(
        __datasource: "withChildren" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Parent[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/Parent/list`);
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let httpResult = HttpResult<Parent[]>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = instantiateObjectArray(httpResult.data, Parent);
    return httpResult;
  }
  static async save(
        model: DeepPartial<Parent>,
        __datasource: "withChildren" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Parent>> {
    const baseUrl = new URL(`http://localhost:5002/api/Parent/save`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            model, 
            __datasource
      })
    });
    let httpResult = HttpResult<Parent>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new Parent(), httpResult.data);
    return httpResult;
  }
}
