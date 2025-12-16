// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody, b64ToU8 } from "cloesce/client";


export class NullabilityChecks {
  id: number;
  notNullableString: string;
  nullableString: string | null;

  async arrayTypes(
    a: number[] | null,
    b: NullabilityChecks[] | null,
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<string[] | null>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/arrayTypes`);
    const payload: any = {};

      payload["a"] = a;
      payload["b"] = b;
      baseUrl.searchParams.append('__dataSource', String(__dataSource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
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
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<NullabilityChecks[] | null | null>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/httpResultTypes`);
    const payload: any = {};

      payload["a"] = a;
      baseUrl.searchParams.append('__dataSource', String(__dataSource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
    });

    return await HttpResult.fromResponse(
      res, 
      MediaType.Json,
      NullabilityChecks,
      true
    );
  }
  async injectableTypes(
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/injectableTypes`);
    const payload: any = {};

      baseUrl.searchParams.append('__dataSource', String(__dataSource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
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
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<NullabilityChecks | null>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/modelTypes`);
    const payload: any = {};

      payload["a"] = a;
      baseUrl.searchParams.append('__dataSource', String(__dataSource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
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
    __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<boolean | null>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/primitiveTypes`);
    const payload: any = {};

      payload["a"] = a;
      payload["b"] = b;
      baseUrl.searchParams.append('__dataSource', String(__dataSource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
      duplex: "half",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, payload)
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
