// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";


export class Horse {
  id: number;
  name: string;
  bio: string | null;
  likes: Like[];

  static async get(
        id: number,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Horse>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/get`);
    baseUrl.searchParams.append('id', String(id));
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new Horse(), raw.data);
    return raw;
  }
  async like(
        horse: Horse,
        __dataSource: "default" |"withLikes" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/${this.id}/like`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            horse, 
            __dataSource
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
  static async list(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Horse[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/list`);
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = instantiateObjectArray(raw.data, Horse);
    return raw;
  }
  static async post(
        horse: Horse,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Horse>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/post`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            horse
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new Horse(), raw.data);
    return raw;
  }
}
export class Like {
  id: number;
  horseId1: number;
  horseId2: number;
  horse2: Horse | undefined;

}
