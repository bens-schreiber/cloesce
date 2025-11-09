// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, instantiateObjectArray, DeepPartial } from "cloesce/client";

export class PooA {
  name: string;
  major: string;
}
export class PooB {
  color: string;
}
export class PooC {
  a: PooA;
  b: PooB;
}

export class PooAcceptYield {
  id: number;

  async acceptPoos(
        a: PooA,
        b: PooB,
        c: PooC,
        __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/PooAcceptYield/${this.id}/acceptPoos`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a, 
            b, 
            c, 
            __dataSource
      })
    });
    let raw = await res.json();
    if (!res.ok) {
      return raw;
    }
    return raw;
  }
  async yieldPoo(
        __dataSource: "none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<PooC>> {
    const baseUrl = new URL(`http://localhost:5002/api/PooAcceptYield/${this.id}/yieldPoo`);
    const res = await fetchImpl(baseUrl, {
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
    raw.data = Object.assign(new PooC(), raw.data);
    return raw;
  }
}
