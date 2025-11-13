// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial } from "cloesce/client";

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
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            dog
      })
    });
    return await HttpResult.fromResponse<Dog>(res, Dog, false);
  }

  static fromJson(data: any): Dog {
    const res = Object.assign(new Dog(), data);
    return res;
  }
}
