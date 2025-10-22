import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";


export class Foo {
  id: number;

  async bar(
        customDs: "baz" |"none" = "none",
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/Foo/${this.id}/bar`);
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            customDs
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
}
