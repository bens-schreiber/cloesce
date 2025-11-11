// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";

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
    let httpResult = HttpResult<Dog>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    httpResult.data = Object.assign(new Dog(), httpResult.data);
    return httpResult;
  }
}
