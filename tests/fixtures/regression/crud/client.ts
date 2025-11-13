// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial } from "cloesce/client";

export class Child {
  id: number;
  parentId: number;
  parent: Parent | undefined;


  static fromJson(data: any): Child {
    const res = Object.assign(new Child(), data);
    res["parent"] &&= Object.assign(new Parent(), res.parent);
    return res;
  }
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
    return await HttpResult.fromResponse(res, CrudHaver, false);
  }
  static async list(
        __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<CrudHaver[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/list`);
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    return await HttpResult.fromResponse(res);
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
    return await HttpResult.fromResponse(res);
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
    return await HttpResult.fromResponse(res, CrudHaver, false);
  }

  static fromJson(data: any): CrudHaver {
    const res = Object.assign(new CrudHaver(), data);
    return res;
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
    return await HttpResult.fromResponse(res, Parent, false);
  }
  static async list(
        __datasource: "withChildren" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Parent[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/Parent/list`);
    baseUrl.searchParams.append('__datasource', String(__datasource));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    return await HttpResult.fromResponse(res);
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
    return await HttpResult.fromResponse(res, Parent, false);
  }

  static fromJson(data: any): Parent {
    const res = Object.assign(new Parent(), data);
    res["favoriteChild"] &&= Object.assign(new Child(), res.favoriteChild);
    for (let i = 0; i < res.children?.length; i++) {
      res.children[i] = Child.fromJson(res.children[i]);
    }
    return res;
  }
}
