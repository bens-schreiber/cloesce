import { HttpResult, instantiateObjectArray } from "cloesce";

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
    dataSource: null = null
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/PooAcceptYield/${this.id}/acceptPoos`);
    if (dataSource) {
      baseUrl.searchParams.append("dataSource", dataSource);
    }
    const res = await fetch(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
            a, 
            b, 
            c
      })
    });

    let raw = await res.json();
    if (!raw.ok) {
      return raw;
    }
    return raw;
  }
  async yieldPoo(
    dataSource: null = null
  ): Promise<HttpResult<PooC>> {
    const baseUrl = new URL(`http://localhost:5002/api/PooAcceptYield/${this.id}/yieldPoo`);
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
    if (!raw.ok) {
      return raw;
    }
    raw.data = Object.assign(new PooC(), raw.data);
    return raw;
  }
}
