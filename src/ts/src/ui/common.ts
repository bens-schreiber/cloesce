import { MediaType } from "../ast.js";

type DeepPartialInner<T> = T extends (infer U)[]
  ? DeepPartialInner<U>[]
  : T extends object
    ? { [K in keyof T]?: DeepPartialInner<T[K]> }
    : T | (null extends T ? null : never);

/**
 * Recursively makes all properties of a type optional — including nested objects and arrays.
 *
 * Similar to TypeScript's built-in `Partial<T>`, but applies the transformation deeply across
 * all nested structures. Useful for defining "patch" or "update" objects where only a subset
 * of properties may be provided.
 *
 * **Apart of the Cloesce method grammar**, meaning the type can be apart of method parameters
 * or return types and the generated workers and client API will act accordingly.
 *
 * @template T
 * The target type to make deeply partial.
 *
 * @remarks
 * - **Objects:** All properties become optional, and their values are recursively wrapped in `DeepPartial`.
 * - **Arrays:** Arrays are preserved, but their elements are recursively made partial.
 * - **Scalars:** Primitive values (string, number, boolean, etc.) remain unchanged.
 * - **Nullable types:** If `null` is assignable to the type, it remains allowed.
 *
 * @example
 * ```ts
 * class User {
 *   id: string;
 *   profile: {
 *     name: string;
 *     age: number;
 *   };
 *   tags: string[];
 * }
 *
 * // The resulting type:
 * // {
 * //   id?: string;
 * //   profile?: { name?: string; age?: number };
 * //   tags?: (string | undefined)[];
 * // }
 * type PartialUser = DeepPartial<User>;
 *
 * const patch: PartialUser = {
 *   profile: { age: 30 } // ok
 * };
 * ```
 */
export type DeepPartial<T> = DeepPartialInner<T> & { __brand?: "Partial" };

/**
 * Base class for a Cloudflare KV model.
 *
 * Consists of a `key`, `value`, and optional `metadata`.
 *
 * @template V The type of the value stored in the KV model. Note that KV is schema-less,
 * so this type is not enforced at runtime, but serves as the type the client expects.
 *
 * @remarks
 * - The `key` is a string that uniquely identifies the entry in the KV store.
 * - The `value` is of generic type `V`, allowing flexibility in the type of data stored.
 * - `V` must be serializable to JSON.
 * - The `metadata` can hold any additional information associated with the KV entry.
 */
export class KVModel<V> {
  key!: string;
  value!: V;
  metadata: unknown;
}

/**
 * Given a media type and some data, converts to a proper
 * `RequestInit` body,
 */
export function requestBody(
  mediaType: MediaType,
  data: any | string | undefined,
): undefined | string | ReadableStream<Uint8Array> {
  switch (mediaType) {
    case MediaType.Json: {
      return JSON.stringify(data ?? {}, (_, v) => {
        // Convert Uint8Arrays to base64 strings
        if (v instanceof Uint8Array) {
          return u8ToB64(v);
        }

        // ReadableStreams are not serializable, toss them
        if (v instanceof ReadableStream) {
          return undefined;
        }

        return v;
      });
    }
    case MediaType.Octet: {
      // JSON structure isn't needed; assume the first
      // value is the stream data
      return Object.values(data)[0] as ReadableStream<Uint8Array>;
    }
  }
}

/**
 * The result of a Workers endpoint.
 *
 * @param ok True if `status` < 400
 * @param status The HTTP Status of a Workers request
 * @param headers All headers that the result is to be sent with or was received with
 * @param data JSON data yielded from a request, undefined if the request was not `ok`.
 * @param message An error text set if the request was not `ok`.
 *
 * @remarks If `status` is 204 `data` will always be undefined.
 *
 */
export class HttpResult<T = unknown> {
  public constructor(
    public ok: boolean,
    public status: number,
    public headers: Headers,
    public data?: T,
    public message?: string,
    public mediaType?: MediaType,
  ) {}

  static ok<T>(status: number, data?: T, init?: HeadersInit): HttpResult {
    const headers: Headers = new Headers(init);
    return new HttpResult<T>(true, status, headers, data, undefined);
  }

  static fail(status: number, message?: string, init?: HeadersInit) {
    const headers: Headers = new Headers(init);
    return new HttpResult<never>(false, status, headers, undefined, message);
  }

  toResponse(): Response {
    switch (this.mediaType) {
      case MediaType.Json: {
        this.headers.set("Content-Type", "application/json");
        break;
      }
      case MediaType.Octet: {
        this.headers.set("Content-Type", "application/octet-stream");
        break;
      }
      case undefined: {
        // Errors are always text.
        this.headers.set("Content-Type", "text/plain");
        return new Response(this.message, {
          status: this.status,
          headers: this.headers,
        });
      }
    }

    return new Response(requestBody(this.mediaType, this.data), {
      status: this.status,
      headers: this.headers,
    });
  }

  setMediaType(mediaType: MediaType): this {
    this.mediaType = mediaType;
    return this;
  }

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
          let json = await response.json();

          if (array) {
            for (let i = 0; i < json.length; i++) {
              json[i] = instantiate(json[i], ctor);
            }
          } else {
            json = instantiate(json, ctor);
          }

          return json;
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

export type Stream = ReadableStream<Uint8Array>;

export function b64ToU8(b64: string): Uint8Array {
  // Prefer Buffer in Node.js environments
  if (typeof Buffer !== "undefined") {
    const buffer = Buffer.from(b64, "base64");
    return new Uint8Array(buffer);
  }

  // Use atob only in browser environments
  const s = atob(b64);
  const u8 = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) {
    u8[i] = s.charCodeAt(i);
  }
  return u8;
}

export function u8ToB64(u8: Uint8Array): string {
  // Prefer Buffer in Node.js environments
  if (typeof Buffer !== "undefined") {
    return Buffer.from(u8).toString("base64");
  }

  // Use btoa only in browser environments
  let s = "";
  for (let i = 0; i < u8.length; i++) {
    s += String.fromCharCode(u8[i]);
  }
  return btoa(s);
}

export type KeysOfType<T, U> = {
  [K in keyof T]: T[K] extends U ? (K extends string ? K : never) : never;
}[keyof T];
