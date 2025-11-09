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
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new CrudHaver(), raw.data);
    return raw;
  }
  static async list(
        __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<CrudHaver[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/list`);
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = instantiateObjectArray(raw.data, CrudHaver);
    return raw;
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
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
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
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new CrudHaver(), raw.data);
    return raw;
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
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new Parent(), raw.data);
    return raw;
  }
  static async list(
        __datasource: "withChildren" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Parent[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/Parent/list`);
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = instantiateObjectArray(raw.data, Parent);
    return raw;
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
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new Parent(), raw.data);
    return raw;
  }
}
