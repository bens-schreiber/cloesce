// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial } from "cloesce/client";
export class Poo {
  ds: "baz" |"none" = "none";

  static fromJson(data: any): Poo {
    const res = Object.assign(new Poo(), data);
    return res;
  }
}


export class Foo {
  id: number;

  async bar(
    customDs: "baz" |"none" = "none",
    oneDs: "default" |"none" = "none",
    noDs: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/Foo/${this.id}/bar`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            customDs, 
            oneDs, 
            noDs
      })
    });
    return await HttpResult.fromResponse<void>(res);
  }

  static fromJson(data: any): Foo {
    const res = Object.assign(new Foo(), data);
    return res;
  }
}
export class NoDs {
  id: number;


  static fromJson(data: any): NoDs {
    const res = Object.assign(new NoDs(), data);
    return res;
  }
}
export class OneDs {
  id: number;


  static fromJson(data: any): OneDs {
    const res = Object.assign(new OneDs(), data);
    return res;
  }
}
