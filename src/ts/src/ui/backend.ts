import { CrudKind, MediaType } from "../ast.js";
import { u8ToB64 } from "../common.js";

/**
 * cloesce/backend
 */
export { CloesceApp, DependencyContainer } from "../router/router.js";
export type { MiddlewareFn, ResultMiddlewareFn } from "../router/router.js";
export type { CrudKind } from "../ast.js";
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
      case MediaType.Json: {
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
      case MediaType.Octet: {
        this.headers.set("Content-Type", "application/octet-stream");

        // JSON structure isn't needed; assume the first
        // value is the stream data
        body = Object.values(this.data ?? {})[0] as BodyInit;
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

export type KeysOfType<T, U> = {
  [K in keyof T]: T[K] extends U ? (K extends string ? K : never) : never;
}[keyof T];

export const Model =
  (_kinds: CrudKind[] = []): ClassDecorator =>
  () => {};

export const Service: ClassDecorator = () => {};

/**
 * Declares a Wrangler environment definition.
 *
 * A `@WranglerEnv` class describes environment bindings
 * available to your Cloudflare Worker at runtime.
 *
 * The environment instance is automatically injected into
 * decorated methods using `@Inject`.
 *
 * Example:
 * ```ts
 * ＠WranglerEnv
 * export class Env {
 *   db: D1Database;
 *   motd: string;
 * }
 *
 * // in a method...
 * foo(＠Inject env: WranglerEnv) {...}
 * ```
 */
export const WranglerEnv: ClassDecorator = () => {};

/**
 * Marks a property as the SQL primary key for a model.
 *
 * Every `@D1` class must define exactly one primary key.
 *
 * Cannot be null.
 *
 * Example:
 * ```ts
 * ＠D1
 * export class User {
 *   ＠PrimaryKey id: number;
 *   name: string;
 * }
 * ```
 */
export const PrimaryKey: PropertyDecorator = () => {};

export const KeyParam: PropertyDecorator = () => {};

export const KV =
  (_keyFormat?: string, _namespaceBinding?: string): PropertyDecorator =>
  () => {};

export const R2 =
  (_keyFormat?: string, _bucketBinding?: string): PropertyDecorator =>
  () => {};

/**
 * Exposes a class method as an HTTP GET endpoint.
 * The method will appear in both backend and generated client APIs.
 */
export const GET: MethodDecorator = () => {};

/**
 * Exposes a class method as an HTTP POST endpoint.
 * The method will appear in both backend and generated client APIs.
 */
export const POST: MethodDecorator = () => {};

/**
 * Exposes a class method as an HTTP PUT endpoint.
 * The method will appear in both backend and generated client APIs.
 */
export const PUT: MethodDecorator = () => {};

/**
 * Exposes a class method as an HTTP PATCH endpoint.
 * The method will appear in both backend and generated client APIs.
 */
export const PATCH: MethodDecorator = () => {};

/**
 * Exposes a class method as an HTTP DEL endpoint.
 * The method will appear in both backend and generated client APIs.
 */
export const DELETE: MethodDecorator = () => {};

export function OneToMany<T>(
  _selector: (model: T) => T[keyof T],
): PropertyDecorator {
  return () => {};
}

export function OneToOne<T>(
  _selector: (model: T) => T[keyof T],
): PropertyDecorator {
  return () => {};
}

export const ForeignKey =
  <T>(_Model: T | string): PropertyDecorator =>
  () => {};

/**
 * Marks a method parameter for dependency injection.
 *
 * Injected parameters can receive environment bindings,
 * middleware-provided objects, or other registered values.
 *
 * Note that injected parameters will not appear in the client
 * API.
 *
 * Example:
 * ```ts
 * ＠POST
 * async neigh(＠Inject env: WranglerEnv) {
 *   return `i am ${this.name}`;
 * }
 * ```
 */
export const Inject: ParameterDecorator = () => {};

type Primitive = string | number | boolean | bigint | symbol | null | undefined;

/**
 * A recursive type describing which related models to include
 * when querying a `＠D1` model.
 *
 * An `IncludeTree<T>` mirrors the shape of the model class,
 * where each navigation property can be replaced with another
 * `IncludeTree` describing nested joins.
 *
 * - Scalar properties (string, number, etc.) are excluded automatically.
 * - Navigation properties (e.g. `dogs: Dog[]`, `owner: Person`) may appear
 *   as keys with empty objects `{}` or nested trees.
 *
 * Example:
 * ```ts
 * ＠D1
 * export class Person {
 *   ＠PrimaryKey id: number;
 *   ＠OneToMany("personId") dogs: Dog[];
 *
 *   ＠DataSource
 *   static readonly default: IncludeTree<Person> = {
 *     dogs: {}, // join Dog table when querying Person
 *   };
 * }
 * ```
 */
export type IncludeTree<T> = (T extends Primitive
  ? never
  : {
      [K in keyof T]?: T[K] extends (infer U)[]
        ? IncludeTree<NonNullable<U>>
        : IncludeTree<NonNullable<T[K]>>;
    }) & { __brand?: "IncludeTree" };

/**
 * Represents the name of a `＠DataSource` available on a model type `T`,
 * or `"none"` when no data source (no joins) should be applied.
 *
 * All instantiated model methods implicitly have a Data Source param `__dataSource`.
 *
 * Example:
 * ```ts
 * ＠D1
 * export class Person {
 *   ＠PrimaryKey id: number;
 *
 *   ＠DataSource
 *   static readonly default: IncludeTree<Person> = { dogs: {} };
 *
 *   ＠POST
 *   foo(ds: DataSourceOf<Person>) {
 *    // Cloesce won't append an implicit data source param here since it's explicit
 *   }
 * }
 *
 * // on the API client:
 * async foo(ds: "default" | "none"): Promise<void> {...}
 * ```
 */
export type DataSourceOf<T extends object> = (
  | KeysOfType<T, IncludeTree<T>>
  | "none"
) & { __brand?: "DataSource" };

/**
 * A branded `number` type indicating that the corresponding
 * SQL column should be created as an `INTEGER`.
 *
 * While all numbers are valid JavaScript types, annotating a
 * field with `Integer` communicates to the Cloesce compiler
 * that this property represents an integer column in SQLite.
 *
 * Example:
 * ```ts
 * ＠D1
 * export class Horse {
 *   ＠PrimaryKey id: Integer;
 *   height: number; // stored as REAL
 * }
 * ```
 */
export type Integer = number & { __brand?: "Integer" };

/**  Hack to detect R2Object at runtime */
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
