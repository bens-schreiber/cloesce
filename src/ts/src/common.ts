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
