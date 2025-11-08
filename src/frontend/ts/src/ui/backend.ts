import { D1Database } from "@cloudflare/workers-types/experimental/index.js";
import { CrudKind, Either, KeysOfType, left, right } from "../common.js";
import { RuntimeContainer } from "../router/router.js";
import {
  WasmResource,
  mapSql as mapSql,
  invokeOrmWasm,
} from "../router/wasm.js";

export { cloesce } from "../router/router.js";
export type {
  HttpResult,
  Either,
  DeepPartial,
  InstanceRegistry,
  CrudKind,
} from "../common.js";
export { CloesceApp } from "../common.js";

/**
 * Marks a class as a D1-backed SQL model.
 *
 * Classes annotated with `@D1` are compiled into:
 *  - a D1 table definition (via `cloesce migrate`)
 *  - backend API endpoints (Workers)
 *  - a frontend client API
 *  - Cloudflare Wrangler configurations
 *
 * Each `@D1` class must define exactly one `@PrimaryKey`.
 *
 * Example:
 *```ts
 *  ＠D1
 *  export class Horse {
 *    ＠PrimaryKey id: number;
 *    name: string;
 *  }
 * ```
 */
export const D1: ClassDecorator = () => {};

/**
 * Marks a class as a plain serializable object.
 *
 * `@PlainOldObject` types represent data that can be safely
 * returned from a model method or API endpoint without being
 * treated as a database model.
 *
 * These are often used for DTOs or view models.
 *
 * Example:
 * ```ts
 * ＠PlainOldObject
 * export class CatStuff {
 *   catFacts: string[];
 *   catNames: string[];
 * }
 * ```
 */
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
 * Data sources describe SQL view definitions (joins) for
 * model relationships. They define which related models
 * are automatically included when querying. Data sources
 * can only reference navigation properties, not scalar
 * attributes.
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
 *   // 👇 Defines a data source that joins the related Dog record
 *   ＠DataSource
 *   static readonly default: IncludeTree<Person> = {
 *     dog: {},
 *   };
 * }
 *
 * // The above will generate an SQL view similar to:
 * // CREATE VIEW "Person.default" AS
 * // SELECT
 * //   "Person"."id" AS "id",
 * //   "Person"."dogId" AS "dogId",
 * //   "Dog"."id" AS "dog.id",
 * //   "Dog"."name" AS "dog.name"
 * // FROM "Person"
 * // LEFT JOIN "Dog" ON "Person"."dogId" = "Dog"."id";
 *
 * // When queried via the ORM or client API:
 * const orm = Orm.fromD1(env.db);
 * const people = (await orm.list(Person, "default")).value;
 * // Each Person instance will now include a populated .dog property.
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
  (_: string): PropertyDecorator =>
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
  (_: string): PropertyDecorator =>
  () => {};

/**
 * Declares a many-to-many relationship between models.
 *
 * The argument is a unique identifier for the generated
 * junction table used to connect the two entities.
 *
 * Example:
 * ```ts
 * ＠ManyToMany("StudentsCourses")
 * courses: Course[];
 * ```
 */
export const ManyToMany =
  (_: string): PropertyDecorator =>
  () => {};

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
  <T>(_: T | string): PropertyDecorator =>
  () => {};

/**
 * Marks a method parameter for dependency injection.
 *
 * Injected parameters can receive environment bindings,
 * middleware-provided objects, or other registered values.
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
 * Supported kinds:
 * - **"SAVE"** — Performs an *upsert* (insert or update) for a model instance.
 * - **"GET"** — Retrieves a single record by its primary key, optionally using a `DataSource`.
 * - **"LIST"** — Retrieves all records for the model, using the specified `DataSource`.
 * - **(future)** `"DELETE"` — Will remove a record by primary key once implemented.
 *
 * The generated methods are static, exposed through both the backend
 * (Worker endpoints) and the frontend client API.
 *
 * Example:
 * ```ts
 * ＠CRUD(["SAVE", "GET", "LIST"])
 * ＠D1
 * export class CrudHaver {
 *   ＠PrimaryKey id: number;
 *   name: string;
 * }
 *
 * // Generated methods (conceptually):
 * // static async save(item: CrudHaver): Promise<HttpResult<CrudHaver>>
 * // static async get(id: number, dataSource?: string): Promise<HttpResult<CrudHaver>>
 * // static async list(dataSource?: string): Promise<HttpResult<CrudHaver[]>>
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
 * This type is used by ORM and CRUD methods to restrict valid
 * data source names to the actual static properties declared on the model.
 *
 * Example:
 * ```ts
 * ＠D1
 * export class Person {
 *   ＠PrimaryKey id: number;
 *
 *   ＠DataSource
 *   static readonly default: IncludeTree<Person> = { dogs: {} };
 * }
 *
 * type DS = DataSourceOf<Person>;
 * // => "default" | "none"
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

/**
 * Provides helper methods for performing ORM operations against a D1 database.
 *
 * The `Orm` class uses the Cloesce metadata system to generate, execute,
 * and map SQL queries for model classes decorated with `＠D1`.
 *
 * Typical operations include:
 * - `fromD1(db)` — create an ORM instance bound to a `D1Database`
 * - `upsert()` — insert or update a model
 * - `list()` — fetch all instances of a model
 * - `get()` — fetch one instance by primary key
 *
 * Example:
 * ```ts
 * const orm = Orm.fromD1(env.db);
 * const horses = (await orm.list(Horse, "default")).value;
 * ```
 */
export class Orm {
  private constructor(private db: D1Database) {}

  /**
   * Creates an instance of an `Orm`
   * @param db The database to use for ORM calls.
   */
  static fromD1(db: D1Database): Orm {
    return new Orm(db);
  }

  /**
   * Maps SQL records to an instantiated Model. The records must be flat
   * (e.g., of the form "id, name, address") or derive from a Cloesce data source view
   * (e.g., of the form "Horse.id, Horse.name, Horse.address")
   * @param ctor The model constructor
   * @param records D1 Result records
   * @param includeTree Include tree to define the relationships to join.
   */
  static mapSql<T extends object>(
    ctor: new () => T,
    records: Record<string, any>[],
    includeTree: IncludeTree<T> | null,
  ): Either<string, T[]> {
    return mapSql(ctor, records, includeTree);
  }

  /**
   * Returns a SQL query to insert a model into the database. Uses an IncludeTree as a guide for
   * foreign key relationships, only inserting the explicitly stated pattern in the tree.
   *
   * TODO: We should be able to leave primary keys and foreign keys undefined, with
   * primary keys being auto incremented and foreign keys being assumed by navigation property
   * context.
   *
   * @param ctor A model constructor.
   * @param newModel The new model to insert.
   * @param includeTree An include tree describing which foreign keys to join.
   * @returns Either an error string, or the insert query string.
   */
  static upsertQuery<T extends object>(
    ctor: new () => T,
    newModel: T,
    includeTree: IncludeTree<T> | null,
  ): Either<string, string> {
    const { wasm } = RuntimeContainer.get();
    const args = [
      WasmResource.fromString(ctor.name, wasm),
      WasmResource.fromString(JSON.stringify(newModel), wasm),
      WasmResource.fromString(JSON.stringify(includeTree), wasm),
    ];
    return invokeOrmWasm(wasm.upsert_model, args, wasm);
  }

  /**
   * Executes an "upsert" query, adding or augmenting a model in the database.
   * If a model's primary key is not defined in `newModel`, the query is assumed to be an insert.
   * If a model's primary key _is_ defined, but some attributes are missing, the query is assumed to be an update.
   * Finally, if the primary key is defined, but all attributes are included, a SQLite upsert will be performed.
   *
   * Capable of inferring foreign keys from the surrounding context of the model. A missing primary key is allowed
   * only if the primary key is an integer, in which case it will be auto incremented and assigned.
   *
   * ### Inserting a new Model
   * ```ts
   * const model = {name: "julio", lastname: "pumpkin"};
   * const idRes = await orm.upsert(Person, model, null);
   * ```
   *
   * ### Updating an existing model
   * ```ts
   * const model =  {id: 1, name: "timothy"};
   * const idRes = await orm.upsert(Person, model, null);
   * // (in db)=> {id: 1, name: "timothy", lastname: "pumpkin"}
   * ```
   *
   * ### Upserting a model
   * ```ts
   * // (assume a Person already exists)
   * const model = {
   *  id: 1,
   *  lastname: "burger", // updates last name
   *  dog: {
   *    name: "fido" // insert dog relationship
   *  }
   * };
   * const idRes = await orm.upsert(Person, model, null);
   * // (in db)=> Person: {id: 1, dogId: 1 ...}  ; Dog: {id: 1, name: "fido"}
   * ```
   *
   * @param ctor A model constructor.
   * @param newModel The new or augmented model.
   * @param includeTree An include tree describing which foreign keys to join.
   * @returns An error string, or the primary key of the inserted model.
   */
  async upsert<T extends object>(
    ctor: new () => T,
    newModel: T,
    includeTree: IncludeTree<T> | null,
  ): Promise<Either<string, any>> {
    let upsertQueryRes = Orm.upsertQuery(ctor, newModel, includeTree);
    if (!upsertQueryRes.ok) {
      return upsertQueryRes;
    }

    // Split the query into individual statements.
    const statements = upsertQueryRes.value
      .split(";")
      .map((s) => s.trim())
      .filter((s) => s.length > 0);

    // One of these statements is a "SELECT", which is the root model id stmt.
    let selectIndex: number;
    for (let i = statements.length - 1; i >= 0; i--) {
      if (/^SELECT/i.test(statements[i])) {
        selectIndex = i;
        break;
      }
    }

    // Execute all statements in a batch.
    const batchRes = await this.db.batch(
      statements.map((s) => this.db.prepare(s)),
    );

    if (!batchRes.every((r) => r.success)) {
      const failed = batchRes.find((r) => !r.success);
      return left(
        failed?.error ?? "D1 batch failed, but no error was returned.",
      );
    }

    // Return the result of the SELECT statement
    const selectResult = batchRes[selectIndex!].results[0] as { id: any };

    return right(selectResult.id);
  }

  /**
   * Returns a query of the form `SELECT * FROM [Model.DataSource]`
   */
  static listQuery<T extends object>(
    ctor: new () => T,
    includeTree: KeysOfType<T, IncludeTree<T>> | null,
  ): string {
    if (includeTree) {
      return `SELECT * FROM [${ctor.name}.${includeTree.toString()}]`;
    }

    return `SELECT * FROM [${ctor.name}]`;
  }

  /**
   * Returns a query of the form `SELECT * FROM [Model.DataSource] WHERE [PrimaryKey] = ?`.
   * Requires the id parameter to be bound (use db.prepare().bind)
   */
  static getQuery<T extends object>(
    ctor: new () => T,
    includeTree: KeysOfType<T, IncludeTree<T>> | null,
  ): string {
    const { ast } = RuntimeContainer.get();
    if (includeTree) {
      return `${this.listQuery(ctor, includeTree)} WHERE [${ast.models[ctor.name].primary_key.name}] = ?`;
    }

    return `${this.listQuery(ctor, includeTree)} WHERE [${ast.models[ctor.name].primary_key.name}] = ?`;
  }

  /**
   * Executes a query of the form `SELECT * FROM [Model.DataSource]`, returning all results
   * as instantiated models.
   */
  async list<T extends object>(
    ctor: new () => T,
    includeTreeKey: KeysOfType<T, IncludeTree<T>> | null,
  ): Promise<Either<string, T[]>> {
    const q = Orm.listQuery(ctor, includeTreeKey);
    const res = await this.db.prepare(q).run();

    if (!res.success) {
      return left(res.error ?? "D1 failed but no error was returned.");
    }

    const { ast } = RuntimeContainer.get();
    const includeTree =
      includeTreeKey === null
        ? null
        : ast.models[ctor.name].data_sources[includeTreeKey.toString()].tree;

    const fromSqlRes = mapSql<T>(ctor, res.results, includeTree);
    if (!fromSqlRes.ok) {
      return fromSqlRes;
    }

    return right(fromSqlRes.value);
  }

  /**
   * Executes a query of the form `SELECT * FROM [Model.DataSource] WHERE [Model.PrimaryKey] = ?`
   * returning all results as instantiated models.
   */
  async get<T extends object>(
    ctor: new () => T,
    id: any,
    includeTreeKey: KeysOfType<T, IncludeTree<T>> | null,
  ): Promise<Either<string, T>> {
    const q = Orm.getQuery(ctor, includeTreeKey);
    const res = await this.db.prepare(q).bind(id).run();

    if (!res.success) {
      return left(res.error ?? "D1 failed but no error was returned.");
    }

    const { ast } = RuntimeContainer.get();
    const includeTree =
      includeTreeKey === null
        ? null
        : ast.models[ctor.name].data_sources[includeTreeKey.toString()].tree;

    const fromSqlRes = mapSql<T>(ctor, res.results, includeTree);
    if (!fromSqlRes.ok) {
      return fromSqlRes;
    }

    return right(fromSqlRes.value[0]);
  }
}
