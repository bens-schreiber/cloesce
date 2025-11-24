// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType } from "cloesce/client";
export class Poo {
  ds: "baz" |"none" = "none";

  static fromJson(data: any, blobs: Uint8Array[]): Poo {
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
      body: JSON.stringify({
            customDs, 
            oneDs, 
            noDs
      })
    });

    return await HttpResult.fromResponse<void>(
      res, 
      MediaType.Json,
    );
  }

  static fromJson(data: any, blobs: Uint8Array[]): Foo {
    const res = Object.assign(new Foo(), data);


    return res;
  }
}
export class NoDs {
  id: number;


  static fromJson(data: any, blobs: Uint8Array[]): NoDs {
    const res = Object.assign(new NoDs(), data);


    return res;
  }
}
export class OneDs {
  id: number;


  static fromJson(data: any, blobs: Uint8Array[]): OneDs {
    const res = Object.assign(new OneDs(), data);


    return res;
  }
}
