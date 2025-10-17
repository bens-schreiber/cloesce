import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";

export class NullabilityChecks {
  id: number;
  notNullableString: string;
  nullableString: string | null;

  async arrayTypes(
        a: number[] | null,
        b: NullabilityChecks[] | null,
    dataSource: null = null
  ): Promise<HttpResult<string[] | null>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/arrayTypes`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a, 
            b
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
    dataSource: null = null
  ): Promise<HttpResult<NullabilityChecks[] | null | null>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/httpResultTypes`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
  async injectableTypes(
    dataSource: null = null
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/injectableTypes`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
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
    dataSource: null = null
  ): Promise<HttpResult<NullabilityChecks | null>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/modelTypes`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a
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
    dataSource: null = null
  ): Promise<HttpResult<number>> {
    const baseUrl = new URL(`http://localhost:5002/api/NullabilityChecks/${this.id}/primitiveTypes`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a, 
            b
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
}
