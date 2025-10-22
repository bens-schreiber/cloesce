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
        dataSource: "none" = "none",
  ): Promise<HttpResult<CrudHaver>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/get`);
    baseUrl.searchParams.append('id', String(id));
    baseUrl.searchParams.append('dataSource', String(dataSource));
    const res = await fetch(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new CrudHaver(), raw.data);
    return raw;
  }
  static async list(
        dataSource: "none" = "none",
  ): Promise<HttpResult<CrudHaver[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/list`);
    baseUrl.searchParams.append('dataSource', String(dataSource));
    const res = await fetch(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = instantiateObjectArray(raw.data, CrudHaver);
    return raw;
  }
  async notCrud(
        __dataSource: "none" = "none",
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/${this.id}/notCrud`);
    const res = await fetch(baseUrl, {
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
  static async patch(
        obj: DeepPartial<CrudHaver>,
        dataSource: "none" = "none",
  ): Promise<HttpResult<CrudHaver>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/patch`);
    const res = await fetch(baseUrl, {
      method: "PATCH",
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
    raw.data = Object.assign(new CrudHaver(), raw.data);
    return raw;
  }
  static async post(
        obj: DeepPartial<CrudHaver>,
        dataSource: "none" = "none",
  ): Promise<HttpResult<CrudHaver>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/post`);
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
        dataSource: "withChildren" |"none" = "none",
  ): Promise<HttpResult<Parent>> {
    const baseUrl = new URL(`http://localhost:5002/api/Parent/get`);
    baseUrl.searchParams.append('id', String(id));
    baseUrl.searchParams.append('dataSource', String(dataSource));
    const res = await fetch(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new Parent(), raw.data);
    return raw;
  }
  static async list(
        dataSource: "withChildren" |"none" = "none",
  ): Promise<HttpResult<Parent[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/Parent/list`);
    baseUrl.searchParams.append('dataSource', String(dataSource));
    const res = await fetch(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = instantiateObjectArray(raw.data, Parent);
    return raw;
  }
  static async patch(
        obj: DeepPartial<Parent>,
        dataSource: "withChildren" |"none" = "none",
  ): Promise<HttpResult<Parent>> {
    const baseUrl = new URL(`http://localhost:5002/api/Parent/patch`);
    const res = await fetch(baseUrl, {
      method: "PATCH",
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
    raw.data = Object.assign(new Parent(), raw.data);
    return raw;
  }
  static async post(
        obj: DeepPartial<Parent>,
        dataSource: "withChildren" |"none" = "none",
  ): Promise<HttpResult<Parent>> {
    const baseUrl = new URL(`http://localhost:5002/api/Parent/post`);
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
    raw.data = Object.assign(new Parent(), raw.data);
    return raw;
  }
}
