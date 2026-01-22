/**
 * Denotes that some error occured internally in Cloesce that should not happen.
 */
export class InternalError extends Error {
  constructor(description: string) {
    super(`An internal Cloesce error occurred: ${description}`);
    Object.setPrototypeOf(this, InternalError.prototype);
  }
}

export class Either<L, R> {
  private constructor(
    private readonly inner: { ok: true; right: R } | { ok: false; left: L },
  ) {}

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
