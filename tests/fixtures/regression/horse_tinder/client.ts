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
    let httpResult = HttpResult<Horse>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new Horse(), httpResult.data);
    return httpResult;
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
    let httpResult = HttpResult<void>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    return httpResult;
  }
  static async list(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Horse[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/list`);
    const res = await fetchImpl(baseUrl, { method: "GET" });
    let httpResult = HttpResult<Horse[]>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = instantiateObjectArray(httpResult.data, Horse);
    return httpResult;
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
    let httpResult = HttpResult<Horse>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new Horse(), httpResult.data);
    return httpResult;
  }
}
export class Like {
  id: number;
  horseId1: number;
  horseId2: number;
  horse2: Horse | undefined;

}
