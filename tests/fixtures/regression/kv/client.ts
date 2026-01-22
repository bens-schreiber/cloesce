// GENERATED CODE. DO NOT MODIFY.


export class D1BackedModel {
  id: number;
  someColumn: number;
  someOtherColumn: string;
  keyParam: string;
  kvData: KValue<unknown>;

  static async get(
    id: number,
    keyParam: string,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<D1BackedModel>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/get`
    );

    baseUrl.searchParams.append("id", String(id));
    baseUrl.searchParams.append("keyParam", String(keyParam));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      D1BackedModel,
      false
    );
  }
  static async list(
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<D1BackedModel[]>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/list`
    );

    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      D1BackedModel,
      true
    );
  }
  static async save(
    model: DeepPartial<D1BackedModel>,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<D1BackedModel>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/save`
    );
    const payload: any = {};

    payload["model"] = model;
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
      D1BackedModel,
      false
    );
  }

  static fromJson(data: any): D1BackedModel {
    const res = Object.assign(new D1BackedModel(), data);
    return res;
  }
}
export class PureKVModel {
  id: string;
  data: KValue<unknown>;
  otherData: KValue<string>;

  static async get(
    id: string,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<PureKVModel>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/PureKVModel/get`
    );

    baseUrl.searchParams.append("id", String(id));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
      duplex: "half",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      PureKVModel,
      false
    );
  }
  static async save(
    model: DeepPartial<PureKVModel>,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<PureKVModel>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/PureKVModel/save`
    );
    const payload: any = {};

    payload["model"] = model;
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
      PureKVModel,
      false
    );
  }

  static fromJson(data: any): PureKVModel {
    const res = Object.assign(new PureKVModel(), data);
    return res;
  }
}

type DeepPartialInner<T> = T extends (infer U)[]
  ? DeepPartialInner<U>[]
  : T extends object
  ? { [K in keyof T]?: DeepPartialInner<T[K]> }
  : T | (null extends T ? null : never);
export type DeepPartial<T> = DeepPartialInner<T> & { __brand?: "Partial" };

export class KValue<V> {
  key!: string;
  raw: unknown | null;
  metadata: unknown | null;
  get value(): V | null {
    return this.raw as V | null;
  }
}

export enum MediaType {
  Json = "Json",
  Octet = "Octet",
}

declare const Buffer: any;
export function b64ToU8(b64: string): Uint8Array {
  if (typeof Buffer !== "undefined") {
    const buffer = Buffer.from(b64, "base64");
    return new Uint8Array(buffer);
  }
  const s = atob(b64);
  const u8 = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) {
    u8[i] = s.charCodeAt(i);
  }
  return u8;
}

export function u8ToB64(u8: Uint8Array): string {
  if (typeof Buffer !== "undefined") {
    return Buffer.from(u8).toString("base64");
  }
  let s = "";
  for (let i = 0; i < u8.length; i++) {
    s += String.fromCharCode(u8[i]);
  }
  return btoa(s);
}

export class R2Object {
  key!: string;
  version!: string;
  size!: number;
  etag!: string;
  httpEtag!: string;
  uploaded!: Date;
  customMetadata?: Record<string, string>;
}

function requestBody(
  mediaType: MediaType,
  data: any | string | undefined,
): BodyInit | undefined {
  switch (mediaType) {
    case MediaType.Json: {
      return JSON.stringify(data ?? {}, (_, v) => {
        if (v instanceof Uint8Array) {
          return u8ToB64(v);
        }
        return v;
      });
    }
    case MediaType.Octet: {
      return Object.values(data)[0] as BodyInit;
    }
  }
}

export class HttpResult<T = unknown> {
  public constructor(
    public ok: boolean,
    public status: number,
    public headers: Headers,
    public data?: T,
    public message?: string,
    public mediaType?: MediaType,
  ) { }

  static async fromResponse(
    response: Response,
    mediaType: MediaType,
    ctor?: any,
    array: boolean = false,
  ): Promise<HttpResult<any>> {
    if (response.status >= 400) {
      return new HttpResult(
        false,
        response.status,
        response.headers,
        undefined,
        await response.text(),
      );
    }

    function instantiate(json: any, ctor?: any) {
      switch (ctor) {
        case Date: {
          return new Date(json);
        }
        case Uint8Array: {
          return b64ToU8(json);
        }
        case undefined: {
          return json;
        }
        default: {
          return ctor.fromJson(json);
        }
      }
    }

    async function data() {
      switch (mediaType) {
        case MediaType.Json: {
          const data = await response.json();

          if (array && Array.isArray(data)) {
            for (let i = 0; i < data.length; i++) {
              data[i] = instantiate(data[i], ctor);
            }
            return data;
          }
          return instantiate(data, ctor);
        }
        case MediaType.Octet: {
          return response.body;
        }
      }
    }
    return new HttpResult(
      true,
      response.status,
      response.headers,
      await data(),
    );
  }
}