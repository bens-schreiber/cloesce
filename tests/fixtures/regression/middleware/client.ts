// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8, KValue, R2Object } from "cloesce/client";

export class InjectedThing {
  value: string;

  static fromJson(data: any): InjectedThing {
    const res = Object.assign(new InjectedThing(), data);
    return res;
  }
}


export class Foo {
  id: number;

  static async blockedMethod(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/Foo/blockedMethod`
    );


    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      undefined,
      false
    );
  }
  static async getInjectedThing(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<InjectedThing>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/Foo/getInjectedThing`
    );


    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      InjectedThing,
      false
    );
  }
  static async save(
    model: DeepPartial<Foo>,
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<Foo>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/Foo/save`
    );
    const payload: any = {};

    payload["model"] = model;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      Foo,
      false
    );
  }

  static fromJson(data: any): Foo {
    const res = Object.assign(new Foo(), data);
    return res;
  }
}
