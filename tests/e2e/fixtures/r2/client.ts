// GENERATED CODE. DO NOT MODIFY.


export class D1BackedModel {
  id: number;
  someColumn: number;
  someOtherColumn: string;
  keyParam: string;
  r2Data: R2Object;

  static async GET(
    id: number,
    keyParam: string,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<D1BackedModel>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/GET`
    );

    baseUrl.searchParams.append("id", String(id));
    baseUrl.searchParams.append("keyParam", String(keyParam));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      D1BackedModel,
      false
    );
  }
  static async LIST(
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<D1BackedModel[]>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/LIST`
    );

    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      D1BackedModel,
      true
    );
  }
  static async SAVE(
    model: DeepPartial<D1BackedModel>,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<D1BackedModel>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/SAVE`
    );
    const payload: any = {};

    payload["model"] = model;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "POST",
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
  async uploadData(
    stream: Uint8Array,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const id = [
      encodeURIComponent(String(this.id)),
      encodeURIComponent(String(this.keyParam)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/D1BackedModel/${id}/uploadData`
    );
    const payload: any = {};

    payload["stream"] = stream;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "PUT",
      headers: { "Content-Type": "application/octet-stream" },
      body: requestBody(MediaType.Octet, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): D1BackedModel {
    const res = Object.assign(new D1BackedModel(), data);
    return res;
  }
}
export class PureR2Model {
  id: string;
  data: R2Object;
  otherData: R2Object;
  allData: R2Object[];

  static async GET(
    id: string,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<PureR2Model>> {
    const baseUrl = new URL(
      `http://localhost:5002/api/PureR2Model/GET`
    );

    baseUrl.searchParams.append("id", String(id));
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "GET",
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      PureR2Model,
      false
    );
  }
  async uploadData(
    stream: Uint8Array,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/PureR2Model/${id}/uploadData`
    );
    const payload: any = {};

    payload["stream"] = stream;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "PUT",
      headers: { "Content-Type": "application/octet-stream" },
      body: requestBody(MediaType.Octet, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      undefined,
      false
    );
  }
  async uploadOtherData(
    stream: Uint8Array,
    __datasource: "default" |"none" = "none",
    fetchImpl: typeof fetch = fetch
  ): Promise<HttpResult<void>> {
    const id = [
      encodeURIComponent(String(this.id)),
    ].join("/");
    const baseUrl = new URL(
      `http://localhost:5002/api/PureR2Model/${id}/uploadOtherData`
    );
    const payload: any = {};

    payload["stream"] = stream;
    baseUrl.searchParams.append("__datasource", String(__datasource));

    const res = await fetchImpl(baseUrl, {
      method: "PUT",
      headers: { "Content-Type": "application/octet-stream" },
      body: requestBody(MediaType.Octet, payload),
    });

    return await HttpResult.fromResponse(
      res,
      MediaType.Json,
      undefined,
      false
    );
  }

  static fromJson(data: any): PureR2Model {
    const res = Object.assign(new PureR2Model(), data);
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
          return response;
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