import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";

export class CrudHaver {
  id: number;
  name: string;

  async notCrud(
    dataSource: null = null
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/${this.id}/notCrud`);
    baseUrl.searchParams.append("dataSource", String(dataSource));
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
  static async post(obj: DeepPartial<CrudHaver>, dataSource: null = null): Promise<HttpResult<CrudHaver>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/POST`);
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

    raw.data = Object.assign(new CrudHaver(), raw.data);
    return raw;
  }
  async patch(dataSource: null = null): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/${this.id}/PATCH`);
    baseUrl.searchParams.append("dataSource", String(dataSource));

    const res = await fetch(baseUrl, {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        obj: this
      })
    });

    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }

    Object.assign(this, raw.data);
    return raw;
  }
  static async get(id: number, dataSource: null = null): Promise<HttpResult<CrudHaver>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/GET`);
    baseUrl.searchParams.append("dataSource", String(dataSource));
    baseUrl.searchParams.append("id", String(id));

    const res = await fetch(baseUrl, { method: "GET" });

    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }

    raw.data = Object.assign(new CrudHaver(), raw.data);
    return raw;
  }
  static async list(dataSource: null = null): Promise<HttpResult<CrudHaver[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/CrudHaver/LIST`);
    baseUrl.searchParams.append("dataSource", String(dataSource));

    const res = await fetch(baseUrl, { method: "GET" });

    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }

    raw.data = instantiateObjectArray(raw.data, CrudHaver);
    return raw;
  }
}
