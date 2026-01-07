// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8, KValue, R2Object } from "cloesce/client";

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
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/Foo/${id}/bar`
    );
    const payload: any = {};

    baseUrl.searchParams.append("customDs", String(customDs));
    baseUrl.searchParams.append("oneDs", String(oneDs));
    baseUrl.searchParams.append("noDs", String(noDs));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      undefined,
      false
    );
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
