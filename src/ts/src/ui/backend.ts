import { KeysOfType } from "./common.js";
import { CrudKind } from "../ast.js";

/**
 * cloesce/backend
 */
export {
  CloesceApp,
  DependencyContainer as DependencyInjector,
} from "../router/router.js";
export type { MiddlewareFn, ResultMiddlewareFn } from "../router/router.js";
export { HttpResult, KValue } from "./common.js";
export type { DeepPartial } from "./common.js";
export type { CrudKind } from "../ast.js";
export { Orm } from "../router/orm.js";

export const Model: ClassDecorator = () => {};
export const Service: ClassDecorator = () => {};
export const PlainOldObject: ClassDecorator = () => {};

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

/**
 * Declares a static property as a data source.
 *
 * Data sources describe SQL left joins related to each
 * models navigation properties.
 *
 * Example:
 * ```ts
 * ＠D1
 * export class Dog {
 *   ＠PrimaryKey
 *   id: number;
 *
 *   name: string;
 * }
 *
 * ＠D1
 * export class Person {
 *   ＠PrimaryKey
 *   id: number;
 *
 *   @ForeignKey(Dog)
 *   dogId: number;
 *
 *   @OneToOne("dogId")
 *   dog: Dog | undefined;
 *
 *   ＠DataSource
 *   static readonly default: IncludeTree<Person> = {
 *     dog: {}, // join Dog table when querying Person with `default` data source
 *   };
 * }
 *
 * // When queried via the ORM or client API:
 * const orm = Orm.fromD1(env.db);
 * const people = await orm.list(Person, Person.default);
 *
 * // => Person { id: 1, dogId: 2, dog: { id: 2, name: "Fido" } }[]
 * ```
 */
export const DataSource: PropertyDecorator = () => {};

/**
 * Declares a one-to-many relationship between models.
 *
 * The argument is the foreign key property name on the
 * related model.
 *
 * Example:
 * ```ts
 * ＠OneToMany("personId")
 * dogs: Dog[];
 * ```
 */
export const OneToMany =
  (_foreignKeyColumn: string): PropertyDecorator =>
  () => {};

/**
 * Declares a one-to-one relationship between models.
 *
 * The argument is the foreign key property name that links
 * the two tables.
 *
 * Example:
 * ```ts
 * ＠OneToOne("dogId")
 * dog: Dog | undefined;
 * ```
 */
export const OneToOne =
  (_foreignKeyColumn: string): PropertyDecorator =>
  () => {};

export const ManyToMany = (): PropertyDecorator => () => {};

/**
 * Declares a foreign key relationship between models.
 * Directly translates to a SQLite foreign key.
 *
 * The argument must reference either a model class or the
 * name of a model class as a string. The property type must
 * match the target model’s primary key type.
 *
 * Example:
 * ```ts
 * ＠ForeignKey(Dog)
 * dogId: number;
 * ```
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
 * ＠POST
 * async neigh(＠Inject env: WranglerEnv) {
 *   return `i am ${this.name}`;
 * }
 * ```
 */
export const Inject: ParameterDecorator = () => {};

/**
 * Enables automatic CRUD method generation for a model.
 *
 * The argument is a list of CRUD operation kinds
 * (e.g. `"SAVE"`, `"GET"`, `"LIST"`) to generate for the model.
 *
 * Cloesce will emit corresponding backend methods and frontend
 * client bindings automatically, removing the need to manually
 * define common API operations.
 *
 * CRUD Operations:
 * - **"SAVE"** — Performs an *upsert* (insert, update, or both) for a model instance.
 * - **"GET"** — Retrieves a single record by its primary key, optionally using a `DataSource`.
 * - **"LIST"** — Retrieves all records for the model, using the specified `DataSource`.
 *
 * The generated methods are static, exposed through both the backend
 * and the frontend client API.
 *
 * Example:
 * ```ts
 * ＠CRUD(["SAVE", "GET", "LIST"])
 * ＠D1
 * export class CrudHaver {
 *   ＠PrimaryKey id: number;
 *   name: string;
 * }
 * ```
 */
export const CRUD =
  (_kinds: CrudKind[]): ClassDecorator =>
  () => {};

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
