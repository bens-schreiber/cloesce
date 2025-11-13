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
    private readonly inner: { ok: true; right: R } | { ok: false; left: L },
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
  ) {}

  static ok<T>(status: number, data?: T, init?: HeadersInit): HttpResult {
    const headers: Headers = new Headers(
      init ?? {
        "Content-Type": "application/json",
      },
    );

    return new HttpResult<T>(true, status, headers, data, undefined);
  }

  static fail(status: number, message?: string, init?: HeadersInit) {
    const headers: Headers = new Headers(
      init ?? {
        "Content-Type": "application/json",
      },
    );

    return new HttpResult<never>(false, status, headers, undefined, message);
  }

  toResponse(): Response {
    const body = () => {
      if (this.status === 204) {
        return undefined;
      }

      return this.ok
        ? JSON.stringify({
            data: this.data,
          })
        : this.message;
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
    ctor?: any,
    array: boolean = false,
  ): Promise<HttpResult<any>> {
    if (response.status < 400) {
      const json = await response.json();

      // TODO: Lazy instantiation via Proxy?
      let data = json.data;
      if (ctor) {
        if (array) {
          for (let i = 0; i < data.length; i++) {
            data[i] = ctor.fromJson(data[i]);
          }
        } else {
          data = ctor.fromJson(data);
        }
      }

      return new HttpResult(true, response.status, response.headers, data);
    }

    return new HttpResult(
      false,
      response.status,
      response.headers,
      undefined,
      await response.text(),
    );
  }
}

export type KeysOfType<T, U> = {
  [K in keyof T]: T[K] extends U ? (K extends string ? K : never) : never;
}[keyof T];
