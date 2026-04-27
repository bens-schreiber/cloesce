import type { D1Result } from "@cloudflare/workers-types";

/**
 * @internal
 * Denotes that some error occured internally in Cloesce that should not happen.
 */
export class InternalError extends Error {
  constructor(description: string) {
    super(`An internal Cloesce error occurred: ${description}`);
    Object.setPrototypeOf(this, InternalError.prototype);
  }
}

/** @internal */
export class Either<L, R> {
  private constructor(private readonly inner: { ok: true; right: R } | { ok: false; left: L }) {}

  get value(): L | R {
    return this.inner.ok ? this.inner.right : this.inner.left;
  }

  static left<R>(): Either<void, R>;
  static left<L, R = never>(value: L): Either<L, R>;

  static left<L, R = never>(value?: L): Either<L | void, R> {
    return new Either({ ok: false, left: value as L | void });
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
    return this.inner.ok ? Either.right(fn(this.inner.right)) : Either.left(this.inner.left);
  }

  mapLeft<B>(fn: (val: L) => B): Either<B, R> {
    return this.inner.ok ? Either.right(this.inner.right) : Either.left(fn(this.inner.left));
  }
}

export type CloesceErrorKind =
  | { kind: "cloesce"; message: string }
  | { kind: "d1"; result: D1Result }
  | { kind: "generic"; error: unknown };

export type CloesceResult<T> =
  | { value: null; errors: CloesceErrorKind[] }
  | { value: T; errors: [] };

/**
 * @internal
 *
 * An internal class to raise a user facing `CloesceResult`
 */
export class CloesceError {
  static drain<T>(results: CloesceResult<T>[]): CloesceResult<never> | void {
    const errors = [];
    for (const r of results) {
      if (r.errors.length > 0) {
        errors.push(...r.errors);
      }
    }

    if (errors.length > 0) {
      return { value: null, errors };
    }
  }

  static generic(error: unknown): CloesceResult<never> {
    return { value: null, errors: [{ kind: "generic", error }] };
  }

  static async catchGeneric<T>(fn: () => Promise<T>): Promise<CloesceResult<T>> {
    try {
      return { value: await fn(), errors: [] };
    } catch (e) {
      return CloesceError.generic(e);
    }
  }

  static cloesce(message: string): CloesceResult<never> {
    return { value: null, errors: [{ kind: "cloesce", message }] };
  }

  static d1(result: D1Result): CloesceResult<never> {
    return { value: null, errors: [{ kind: "d1", result }] };
  }

  static displayErrors(result: CloesceResult<never>): string {
    function display(v: unknown): string {
      try {
        return JSON.stringify(v);
      } catch {
        return String(v);
      }
    }

    return result.errors
      .map((e) => {
        switch (e.kind) {
          case "cloesce":
            return e.message;
          case "d1":
            return `A D1 error occurred: ${display(e.result)}`;
          case "generic":
            return `An error occurred: ${display(e.error)}`;
        }
      })
      .join(", ");
  }
}

/** @internal */
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
