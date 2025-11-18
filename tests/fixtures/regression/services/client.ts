// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial } from "cloesce/client";

export class BarService {
  static async useFoo(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<string>> {
    const baseUrl = new URL("http://localhost:5002/api/BarService/useFoo");
    const res = await fetchImpl(baseUrl, { method: "GET" });
    return await HttpResult.fromResponse<string>(res);
    }
}
export class FooService {
  static async instantiatedMethod(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<string>> {
    const baseUrl = new URL("http://localhost:5002/api/FooService/instantiatedMethod");
    const res = await fetchImpl(baseUrl, { method: "GET" });
    return await HttpResult.fromResponse<string>(res);
    }
  static async staticMethod(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<string>> {
    const baseUrl = new URL("http://localhost:5002/api/FooService/staticMethod");
    const res = await fetchImpl(baseUrl, { method: "GET" });
    return await HttpResult.fromResponse<string>(res);
    }
}

