import { CrudKind, MediaType } from "../ast.js";
import { u8ToB64 } from "../common.js";
import { DataSource } from "../router/orm.js";
import { DataSourceContainer } from "../router/router.js";

/**
 * cloesce/backend
 */
export { CloesceApp, DependencyContainer } from "../router/router.js";
export type { MiddlewareFn } from "../router/router.js";
export type { CrudKind } from "../ast.js";
export { Orm, DataSource } from "../router/orm.js";
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

/**
 * Marks a class as a Cloesce Model.
 *
 * A stub decorator used by Cloesce to identify model classes.
 *
 * @param _kinds The CRUD kinds supported by this model.
 */
export const Model =
  (_kinds: CrudKind[] = []): ClassDecorator =>
  () => {};

/**
 * Marks a class as a Cloesce Service.
 *
 * A stub decorator used by Cloesce to identify service classes.
 *
 * @remarks
 * On initialization, Services will call the `init` method if it exists.
 * `init` can be async, but can accept only ＠Inject parameters.
 * `init` must return either void or a HttpResult.
 *
 * Example:
 * ```ts
 * ＠Service
 * export class MyService {
 *   injectedProperty: SomeDependency;
 *
 *   async init(＠Inject env: WranglerEnv) {
 *    // perform async setup here
 *   }
 * }
 * ```
 */
export const Service: ClassDecorator = () => {};

/**
 * Declares a Wrangler environment definition.
 *
 * A `@WranglerEnv` decorated class describes environment bindings
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
 * foo(＠Inject env: Env) {...}
 * ```
 */
export const WranglerEnv: ClassDecorator = () => {};

/**
 * Marks a property as the SQL primary key for a model.
 *
 * Every `@D1` class must define exactly one primary key.
 *
 * A primary key property cannot be nullable.
 *
 * Example:
 * ```ts
 * ＠D1
 * export class User {
 *   ＠PrimaryKey
 *   id: number;
 *
 *   name: string;
 * }
 * ```
 */
export const PrimaryKey: PropertyDecorator = () => {};

/**
 * Marks a property as a key parameter for KV or R2 models.
 *
 * A stub decorator used by Cloesce to identify key properties.
 *
 * Must decorate a string property.
 */
export const KeyParam: PropertyDecorator = () => {};

/**
 * Marks a property as a Cloudflare KV binding.
 *
 * @param _keyFormat A key format string for the KV binding. Uses string interpolation syntax, e.g. `users/${userId}/settings`
 * @param _namespaceBinding The name of the KV namespace binding in the Wrangler environment.
 *
 * @remarks
 * This is a stub decorator used by Cloesce to identify KV bindings.
 * It does not have any runtime behavior.
 */
export const KV =
  (_keyFormat: string, _namespaceBinding: string): PropertyDecorator =>
  () => {};

/**
 * Marks a property as a Cloudflare R2 binding.
 *
 * A stub decorator used by Cloesce to identify R2 bindings.
 *
 * @param _keyFormat A key format string for the R2 binding. Uses string interpolation syntax, e.g. `uploads/${userId}/file.txt`
 * @param _bucketBinding The name of the R2 bucket binding in the Wrangler environment.
 */
export const R2 =
  (_keyFormat: string, _bucketBinding: string): PropertyDecorator =>
  () => {};

/**
 * Exposes a class method as an HTTP GET endpoint.
 * The method will appear in both backend and generated client APIs.
 */
export function Get(dataSource?: DataSource<unknown>): MethodDecorator {
  return function (target, propertyKey) {
    if (dataSource) {
      DataSourceContainer.set(
        target.constructor.name,
        propertyKey.toString(),
        dataSource,
      );
    }
  };
}

/**
 * Exposes a class method as an HTTP POST endpoint.
 * The method will appear in both backend and generated client APIs.
 */
export function Post(dataSource?: DataSource<unknown>): MethodDecorator {
  return function (target, propertyKey) {
    if (dataSource) {
      DataSourceContainer.set(
        target.constructor.name,
        propertyKey.toString(),
        dataSource,
      );
    }
  };
}

/**
 * Exposes a class method as an HTTP PUT endpoint.
 * The method will appear in both backend and generated client APIs.
 */
export function Put(dataSource?: DataSource<unknown>): MethodDecorator {
  return function (target, propertyKey) {
    if (dataSource) {
      DataSourceContainer.set(
        target.constructor.name,
        propertyKey.toString(),
        dataSource,
      );
    }
  };
}

/**
 * Exposes a class method as an HTTP PATCH endpoint.
 * The method will appear in both backend and generated client APIs.
 */
export function Patch(dataSource?: DataSource<unknown>): MethodDecorator {
  return function (target, propertyKey) {
    if (dataSource) {
      DataSourceContainer.set(
        target.constructor.name,
        propertyKey.toString(),
        dataSource,
      );
    }
  };
}

/**
 * Exposes a class method as an HTTP DEL endpoint.
 * The method will appear in both backend and generated client APIs.
 */
export function Del(dataSource?: DataSource<unknown>): MethodDecorator {
  return function (target, propertyKey) {
    if (dataSource) {
      DataSourceContainer.set(
        target.constructor.name,
        propertyKey.toString(),
        dataSource,
      );
    }
  };
}

/**
 * Marks a property as a one-to-many navigation property.
 *
 * A stub decorator used by Cloesce to identify one-to-many relationships.
 *
 * @param _selector A selector function that returns the foreign key property on the related model, e.g. `model => model.ownerId`
 *
 * @template T The type of the model to which the navigation property relates.
 */
export function OneToMany<T>(
  _selector: (model: T) => T[keyof T],
): PropertyDecorator {
  return () => {};
}

/**
 * Marks a property as a one-to-one navigation property.
 *
 * A stub decorator used by Cloesce to identify one-to-one relationships.
 *
 * @param _selector A selector function that returns the foreign key property on this model, e.g. `model => model.profileId`
 *
 * @template T The type of the model containing the navigation property.
 */
export function OneToOne<T>(
  _selector: (model: T) => T[keyof T],
): PropertyDecorator {
  return () => {};
}

/**
 * Marks a property as a foreign key to another model.
 *
 * A stub decorator used by Cloesce to identify foreign key properties.
 *
 * @param _Model The related model class or its name as a string.
 */
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
 * ＠Post
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
 * @template T The model type for which to define the include tree.
 *
 * An `IncludeTree<T>` mirrors the shape of the model class,
 * where each navigation property can be replaced with another
 * `IncludeTree` describing nested joins.
 *
 * - Scalar properties (string, number, etc.) are included automatically.
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
export type IncludeTree<T> = T extends Primitive
  ? never
  : {
      [K in keyof T]?: T[K] extends (infer U)[]
        ? IncludeTree<NonNullable<U>>
        : IncludeTree<NonNullable<T[K]>>;
    };

/**
 * A branded `number` type indicating that the corresponding
 * SQL column should be created as an `INTEGER`.
 *
 * While all numbers are valid JavaScript types, annotating a
 * field with `Integer` communicates to Cloesce
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
