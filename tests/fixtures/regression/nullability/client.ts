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
  ): Promise<HttpResult<string[] | null>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/arrayTypes`);
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a, 
            b, 
            __dataSource
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
  async httpResultTypes(
        a: number | null | null,
        __dataSource: "none" = "none",
  ): Promise<HttpResult<NullabilityChecks[] | null | null>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/httpResultTypes`);
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a, 
            __dataSource
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
  async injectableTypes(
        __dataSource: "none" = "none",
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/injectableTypes`);
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            __dataSource
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
  async modelTypes(
        a: NullabilityChecks | null,
        __dataSource: "none" = "none",
  ): Promise<HttpResult<NullabilityChecks | null>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/modelTypes`);
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a, 
            __dataSource
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
  async primitiveTypes(
        a: number | null,
        b: string | null,
        __dataSource: "none" = "none",
  ): Promise<HttpResult<number>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/primitiveTypes`);
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a, 
            b, 
            __dataSource
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
}
