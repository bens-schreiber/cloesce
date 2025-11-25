// GENERATED CODE. DO NOT MODIFY.

import { HttpResult, DeepPartial, MediaType, requestBody } from "cloesce/client";
export class PooA {
  name: string;
  major: string;

  static fromJson(data: any, blobs: Uint8Array[]): PooA {
    const res = Object.assign(new PooA(), data);
    return res;
  }
}
export class PooB {
  color: string;

  static fromJson(data: any, blobs: Uint8Array[]): PooB {
    const res = Object.assign(new PooB(), data);
    return res;
  }
}
export class PooC {
  a: PooA;
  b: PooB[];

  static fromJson(data: any, blobs: Uint8Array[]): PooC {
    const res = Object.assign(new PooC(), data);
    res["a"] &&= PooA.fromJson(res.a, blobs);
    for (let i = 0; i < res.b?.length; i++) {
      res.b[i] = PooB.fromJson(res.b[i], blobs);
    }
    return res;
  }
}


export class PooAcceptYield {
  id: number;

  static async acceptPoos(
    a: PooA,
    b: PooB,
    c: PooC,
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const baseUrl = new URL(`http://localhost:5002/api/PooAcceptYield/acceptPoos`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, {
            a, 
            b, 
            c
      })
    });

    return await HttpResult.fromResponse<void>(
      res, 
      MediaType.Json,
    );
  }
  static async yieldPoo(
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<PooC>> {
    const baseUrl = new URL(`http://localhost:5002/api/PooAcceptYield/yieldPoo`);
    const res = await fetchImpl(baseUrl, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: requestBody(MediaType.Json, {
      })
    });

    return await HttpResult.fromResponse<PooC>(
      res, 
      MediaType.Json,
      PooC, false
    );
  }

  static fromJson(data: any, blobs: Uint8Array[]): PooAcceptYield {
    const res = Object.assign(new PooAcceptYield(), data);


    return res;
  }
}
