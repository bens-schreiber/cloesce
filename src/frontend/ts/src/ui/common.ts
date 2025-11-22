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

export class Either<L, R> {
  private constructor(
    private readonly inner: { ok: true; right: R } | { ok: false; left: L }
  ) {}

  get value(): L | R {
    return this.inner.ok ? this.inner.right : this.inner.left;
  }

  static left<L, R = never>(value: L): Either<L, R> {
    return new Either({ ok: false, left: value });
  }

  static right<R, L = never>(value: R): Either<L, R> {
    return new Either({ ok: true, right: value });
  }

  isLeft(): this is Either<L, never> {
    return !this.inner.ok;
  }

  isRight(): this is Either<never, R> {
    return this.inner.ok;
  }

  unwrap(): R {
    if (!this.inner.ok) {
      throw new Error("Tried to unwrap a Left value");
    }
    return this.inner.right;
  }

  unwrapLeft(): L {
    if (this.inner.ok) {
      throw new Error("Tried to unwrapLeft a Right value");
    }
    return this.inner.left;
  }

  map<B>(fn: (val: R) => B): Either<L, B> {
    return this.inner.ok
      ? Either.right(fn(this.inner.right))
      : Either.left(this.inner.left);
  }

  mapLeft<B>(fn: (val: L) => B): Either<B, R> {
    return this.inner.ok
      ? Either.right(this.inner.right)
      : Either.left(fn(this.inner.left));
  }
}

const contentTypeMap: Record<MediaType, string> = {
  [MediaType.Json]: "application/json",
  [MediaType.FormData]: "multipart/form-data",
  [MediaType.Octet]: "application/octet-stream",
};

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
    public mediaType: MediaType,
    public data?: T,
    public message?: string
  ) {}

  static ok<T>(
    status: number,
    data?: T,
    init?: HeadersInit,
    mediaType: MediaType = MediaType.Json
  ): HttpResult {
    const headers: Headers = new Headers(
      init ?? {
        "Content-Type": contentTypeMap[mediaType],
      }
    );

    return new HttpResult<T>(true, status, headers, mediaType, data, undefined);
  }

  static fail(status: number, message?: string, init?: HeadersInit) {
    const headers: Headers = new Headers(
      init ?? {
        "Content-Type": "text/plain",
      }
    );

    return new HttpResult<never>(
      false,
      status,
      headers,
      MediaType.Json, // default, won't be used
      undefined,
      message
    );
  }

  toResponse(): Response {
    const body = () => {
      // No body
      if (this.status === 204) {
        return undefined;
      }

      // Failures will always return as text.
      if (!this.ok) {
        return this.message;
      }

      switch (this.mediaType) {
        case MediaType.Json: {
          return JSON.stringify(this.data);
        }
        case MediaType.FormData: {
          const formData = new FormData();
          let blobIndex = 0;

          const json = JSON.stringify(this.data, (key, value) => {
            if (value instanceof Blob) {
              const index = blobIndex++;
              formData.append("blobs[]", value, String(index));
              return { __blobIndex: index };
            }

            return value;
          });

          formData.set("json", json);

          return formData;
        }
        case MediaType.Octet: {
          return this.data as Blob | ArrayBuffer | ReadableStream;
        }
      }
    };

    return new Response(body(), {
      status: this.status,
      headers: this.headers,
    });
  }

  /**
   * @internal
   * A method utilized by generated client code to create an `HttpResult` from a Cloesce Workers
   * `Response`. Given a ctor, assumes it is a Plain old Object or a Model.
   *
   * All Cloesce objects have a `static fromJson` method which recursively instantiate the object.
   */
  static async fromResponse(
    response: Response,
    mediaType: MediaType,
    ctor?: any,
    array: boolean = false
  ): Promise<HttpResult<any>> {
    if (response.status > 400) {
      return new HttpResult(
        false,
        response.status,
        response.headers,
        MediaType.Json,
        undefined,
        await response.text()
      );
    }

    const data = async () => {
      switch (mediaType) {
        case MediaType.Json: {
          let json = await response.json();

          if (!ctor) {
            return json;
          }

          if (array) {
            for (let i = 0; i < json.length; i++) {
              json[i] = ctor.fromJson(json[i]);
            }
          } else {
            json = ctor.fromJson(json);
          }

          return json;
        }

        case MediaType.FormData: {
          // todo: blob[]?
          const formData = await response.formData();
          const blobs = formData.getAll("blobs[]");
          let json: any = formData.get("json")!;

          if (array) {
            for (let i = 0; i < json.length; i++) {
              json[i] = ctor.fromJson(json[i], blobs);
            }
          } else {
            json = ctor.fromJson(json, blobs);
          }

          return json;
        }

        case MediaType.Octet: {
          const buffer = await response.arrayBuffer();
          return new Blob([buffer]);
        }
      }
    };

    return new HttpResult(
      true,
      response.status,
      response.headers,
      mediaType,
      await data()
    );
  }
}

export type KeysOfType<T, U> = {
  [K in keyof T]: T[K] extends U ? (K extends string ? K : never) : never;
}[keyof T];
