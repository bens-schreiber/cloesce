// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8 } from "cloesce/client";


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

    return await HttpResult.fromResponse<Horse>(
      res, 
      MediaType.Json,
      Horse,
      false
    );
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
      body: requestBody(MediaType.Json, {
            horse, 
            __dataSource
      })
    });

    return await HttpResult.fromResponse<void>(
      res, 
      MediaType.Json,
      undefined,
      false
    );
  }
  static async list(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Horse[]>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/list`);
    const res = await fetchImpl(baseUrl, { method: "GET" });

    return await HttpResult.fromResponse<Horse[]>(
      res, 
      MediaType.Json,
      Horse,
      true
    );
  }
  static async post(
    horse: Horse,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Horse>> {
    const baseUrl = new URL(`http://localhost:5002/api/Horse/post`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, {
            horse
      })
    });

    return await HttpResult.fromResponse<Horse>(
      res, 
      MediaType.Json,
      Horse,
      false
    );
  }

  static fromJson(data: any): Horse {
    const res = Object.assign(new Horse(), data);
    for (let i = 0; i < res.likes?.length; i++) {
      res.likes[i] = Like.fromJson(res.likes[i]);
    }


    return res;
  }
}
export class Like {
  id: number;
  horseId1: number;
  horseId2: number;
  horse2: Horse | undefined;


  static fromJson(data: any): Like {
    const res = Object.assign(new Like(), data);
    res["horse2"] &&= Horse.fromJson(res.horse2);


    return res;
  }
}
