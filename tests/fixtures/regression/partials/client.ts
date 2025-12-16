// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8 } from "cloesce/client";


export class Dog {
  id: number;
  name: string;
  age: number;

  async getPartialSelf(
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<DeepPartial<Dog>>> {
    const baseUrl = new URL(`http://localhost:5002/api/Dog/${this.id}/getPartialSelf`);

    baseUrl.searchParams.append('__dataSource', String(__dataSource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      DeepPartial<Dog>,
      false
    );
  }
  static async post(
    dog: DeepPartial<Dog>,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Dog>> {
    const baseUrl = new URL(`http://localhost:5002/api/Dog/post`);
    const payload: any = {};

    payload["dog"] = dog;

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      Dog,
      false
    );
  }

  static fromJson(data: any): Dog {
    const res = Object.assign(new Dog(), data);
    return res;
  }
}
