import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";


export class Dog {
  id: number;
  name: string;
  age: number;

  static async post(
        dog: DeepPartial<Dog>,
  ): Promise<HttpResult<Dog>> {
    const baseUrl = new URL(`http://localhost:5002/api/Dog/post`);
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            dog
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    raw.data = Object.assign(new Dog(), raw.data);
    return raw;
  }
}
