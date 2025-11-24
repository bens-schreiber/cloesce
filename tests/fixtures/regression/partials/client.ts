// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType } from "cloesce/client";


export class Dog {
  id: number;
  name: string;
  age: number;

  static async post(
    dog: DeepPartial<Dog>,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Dog>> {
    const baseUrl = new URL(`http://localhost:5002/api/Dog/post`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      body: JSON.stringify({
            dog
      })
    });

    return await HttpResult.fromResponse<Dog>(
      res, 
      MediaType.Json,
      Dog, false
    );
  }

  static fromJson(data: any, blobs: Uint8Array[]): Dog {
    const res = Object.assign(new Dog(), data);


    return res;
  }
}
