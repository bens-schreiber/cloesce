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
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, {
            a, 
            b, 
            __dataSource
      })
    });

    return await HttpResult.fromResponse<string[] | null>(
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
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, {
            a, 
            __dataSource
      })
    });

    return await HttpResult.fromResponse<NullabilityChecks[] | null | null>(
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
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, {
            __dataSource
      })
    });

    return await HttpResult.fromResponse<void>(
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
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, {
            a, 
            __dataSource
      })
    });

    return await HttpResult.fromResponse<NullabilityChecks | null>(
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
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, {
            a, 
            b, 
            __dataSource
      })
    });

    return await HttpResult.fromResponse<boolean | null>(
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
