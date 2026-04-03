import type { MediaType } from "../cidl.js";
import { u8ToB64 } from "../common.js";

/**
 * cloesce/backend
 */
export { CloesceApp, DependencyContainer } from "../router/router.js";
export type { MiddlewareFn } from "../router/router.js";
export type { CrudKind } from "../cidl.js";
export { Orm } from "../router/orm.js";
export { R2ObjectBody } from "@cloudflare/workers-types";

/**
 * Base class for a Cloudflare KV model or navigation property.
 *
 * Consists of a `key`, `value`, and optional `metadata`.
 *
 * @template V The type of the value stored in the KValue. Note that KV is schema-less,
 * so this type is not enforced at runtime, but serves as the type the client expects.
 *
 * @remarks
 * - The `key` is a string that uniquely identifies the entry in the KV store.
 * - The `value` is of generic type `V`, allowing flexibility in the type of data stored.
 * - `V` must be serializable to JSON.
 * - The `metadata` can hold any additional information associated with the KV entry.
 */
export class KValue<V> {
  key!: string;
  raw: unknown | null;
  metadata: unknown | null;

  get value(): V | null {
    return this.raw as V | null;
  }
}

export type Primitive = string | number | boolean | bigint | symbol | null | undefined;
export type IncludeTree<T> = T extends Primitive
  ? never
  : {
    [K in keyof T]?: T[K] extends (infer U)[]
    ? IncludeTree<NonNullable<U>>
    : IncludeTree<NonNullable<T[K]>>;
  };

export interface Paginated<T> {
  results: T[];
  cursor: string | null;
  complete: boolean;
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
 * @template T The type of `data` returned on success.
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
  ) { }

  static ok<T>(status: number, data?: T, init?: HeadersInit): HttpResult<T> {
    const headers: Headers = new Headers(init);
    return new HttpResult<T>(true, status, headers, data, undefined);
  }

  static fail(status: number, message?: string, init?: HeadersInit) {
    const headers: Headers = new Headers(init);
    return new HttpResult<never>(false, status, headers, undefined, message);
  }

  toResponse(): Response {
    let body: BodyInit;
    switch (this.mediaType) {
      case "Json": {
        this.headers.set("Content-Type", "application/json");
        body = JSON.stringify(this.data ?? {}, (_, v) => {
          // Convert Uint8Arrays to base64 strings
          if (v instanceof Uint8Array) {
            return u8ToB64(v);
          }

          // Convert R2Object to Client R2Object representation
          if (isR2Object(v)) {
            return {
              key: v.key,
              version: v.version,
              size: v.size,
              etag: v.etag,
              httpEtag: v.httpEtag,
              uploaded: v.uploaded.toISOString(),
              customMetadata: v.customMetadata,
            };
          }

          if (v instanceof Date) {
            return v.toISOString();
          }

          return v;
        });
        break;
      }
      case "Octet": {
        this.headers.set("Content-Type", "application/octet-stream");

        // Assume proper BodyInit
        body = this.data as BodyInit;
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

    return new Response(body, {
      status: this.status,
      headers: this.headers,
    });
  }

  setMediaType(mediaType: MediaType): this {
    this.mediaType = mediaType;
    return this;
  }
}

type DeepPartialInner<T> = T extends (infer U)[]
  ? DeepPartialInner<U>[]
  : T extends object
  ? { [K in keyof T]?: DeepPartialInner<T[K]> }
  : T | (null extends T ? null : never);

/**
 * Recursively makes all properties of a type optional, including nested objects and arrays.
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

export type KeysOfType<T, U> = {
  [K in keyof T]: T[K] extends U ? (K extends string ? K : never) : never;
}[keyof T];

/**  @internal Hack to detect R2Object at runtime */
function isR2Object(x: unknown): boolean {
  if (typeof x !== "object" || x === null) return false;
  const o = x as any;
  return (
    typeof o.key === "string" &&
    typeof o.version === "string" &&
    typeof o.size === "number" &&
    typeof o.etag === "string" &&
    typeof o.httpEtag === "string" &&
    typeof o.uploaded === "object" &&
    typeof o.uploaded?.getTime === "function" &&
    typeof o.storageClass === "string" &&
    typeof o.writeHttpMetadata === "function"
  );
}
