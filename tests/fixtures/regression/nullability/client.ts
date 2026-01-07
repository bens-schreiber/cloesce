// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8, KValue, R2Object } from "cloesce/client";



export class NullabilityChecks {
  id: number;
  notNullableString: string;
  nullableString: string | null;

  async arrayTypes(
    a: number[] | null,
    b: NullabilityChecks[] | null,
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<string[] | null>> {
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/NullabilityChecks/${id}/arrayTypes`
    );
    const payload: any = {};

    payload["a"] = a;
    payload["b"] = b;
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
      undefined,
      true
    );
  }
  async httpResultTypes(
    a: number | null | null,
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<NullabilityChecks[] | null | null>> {
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/NullabilityChecks/${id}/httpResultTypes`
    );
    const payload: any = {};

    payload["a"] = a;
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
      NullabilityChecks,
      true
    );
  }
  async injectableTypes(
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/NullabilityChecks/${id}/injectableTypes`
    );
    const payload: any = {};

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
      undefined,
      false
    );
  }
  async modelTypes(
    a: NullabilityChecks | null,
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<NullabilityChecks | null>> {
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/NullabilityChecks/${id}/modelTypes`
    );
    const payload: any = {};

    payload["a"] = a;
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
      NullabilityChecks,
      false
    );
  }
  async primitiveTypes(
    a: number | null,
    b: string | null,
    __datasource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<boolean | null>> {
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/NullabilityChecks/${id}/primitiveTypes`
    );
    const payload: any = {};

    payload["a"] = a;
    payload["b"] = b;
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
      undefined,
      false
    );
  }

  static fromJson(data: any): NullabilityChecks {
    const res = Object.assign(new NullabilityChecks(), data);
    return res;
  }
}
