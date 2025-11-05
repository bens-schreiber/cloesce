// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";

export class Poo {
  ds: "baz" |"none" = "none";
}

export class Foo {
  id: number;

  async bar(
        customDs: "baz" |"none" = "none",
        oneDs: "default" |"none" = "none",
        noDs: "none" = "none",
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/Foo/${this.id}/bar`);
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            customDs, 
            oneDs, 
            noDs
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
}
export class NoDs {
  id: number;

}
export class OneDs {
  id: number;

}
