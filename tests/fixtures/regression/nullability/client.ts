// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";

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
      body: JSON.stringify({
            a, 
            b, 
            __dataSource
      })
    });
    let httpResult = HttpResult<string[] | null>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    return httpResult;
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
      body: JSON.stringify({
            a, 
            __dataSource
      })
    });
    let httpResult = HttpResult<NullabilityChecks[] | null | null>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    return httpResult;
  }
  async injectableTypes(
        __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/injectableTypes`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            __dataSource
      })
    });
    let httpResult = HttpResult<void>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    return httpResult;
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
      body: JSON.stringify({
            a, 
            __dataSource
      })
    });
    let httpResult = HttpResult<NullabilityChecks | null>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    return httpResult;
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
      body: JSON.stringify({
            a, 
            b, 
            __dataSource
      })
    });
    let httpResult = HttpResult<boolean | null>.fromJSON(await res.json());
    if (!res.ok) {
      return httpResult;
    }
    return httpResult;
  }
}
