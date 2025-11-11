import { D1Database } from "@cloudflare/workers-types/experimental/index.js";
import { CrudKind, DeepPartial, Either, KeysOfType } from "../common.js";
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
 * Data sources describe SQL CTE definitions (joins) for
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
 *   // Defines a data source that joins the related Dog record
 *   ＠DataSource
 *   static readonly default: IncludeTree<Person> = {
 *     dog: {},
 *   };
 * }
 *
 * // When queried via the ORM or client API:
 * const orm = Orm.fromD1(env.db);
 * const people = (await orm.list(Person, Person.default)).value;
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
    includeTree: IncludeTree<T> | null = null
  ): Either<string, T[]> {
    return mapSql(ctor, records, includeTree);
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
    newModel: DeepPartial<T>,
    includeTree: IncludeTree<T> | null = null
  ): Promise<Either<string, any>> {
    const { wasm } = RuntimeContainer.get();
    const args = [
      WasmResource.fromString(ctor.name, wasm),
      WasmResource.fromString(JSON.stringify(newModel), wasm),
      WasmResource.fromString(JSON.stringify(includeTree), wasm),
    ];

    const upsertQueryRes = invokeOrmWasm(wasm.upsert_model, args, wasm);
    if (upsertQueryRes.isLeft()) {
      return upsertQueryRes;
    }

    const statements = JSON.parse(upsertQueryRes.unwrap()) as {
      query: string;
      values: any[];
    }[];

    // One of these statements is a "SELECT", which is the root model id stmt.
    let selectIndex: number;
    for (let i = statements.length - 1; i >= 0; i--) {
      if (/^SELECT/i.test(statements[i].query)) {
        selectIndex = i;
        break;
      }
    }

    // Execute all statements in a batch.
    const batchRes = await this.db.batch(
      statements.map((s) => this.db.prepare(s.query).bind(...s.values))
    );

    if (!batchRes.every((r) => r.success)) {
      const failed = batchRes.find((r) => !r.success);
      return Either.left(
        failed?.error ?? "D1 batch failed, but no error was returned."
      );
    }

    // Return the result of the SELECT statement
    const selectResult = batchRes[selectIndex!].results[0] as { id: any };

    return Either.right(selectResult.id);
  }

  /**
   * Returns a query using the provided include tree, aliasing columns to match the
   * object structure. Wraps the query in a CTE.
   *
   * @param ctor The model constructor.
   * @param includeTree An include tree describing which related models to join.
   * @param from An optional custom `FROM` clause to use instead of the base table.
   * @param tagCte An optional CTE name to tag the query with. Defaults to "Model.view".
   *
   * Example:
   * ```ts
   * // Using a data source
   * const query = Orm.listQuery(Person, "default");
   *
   * // Using a custom from statement
   * const query = Orm.listQuery(Person, null, "SELECT * FROM Person WHERE age > 18");
   * ```
   * Example SQL output:
   * ```sql
   * WITH Person_view AS (
   * SELECT
   * "Person"."id" AS "id",
   * ...
   * FROM "Person"
   * LEFT JOIN ...
   * )
   * SELECT * FROM Person_view
   * ```
   */
  static listQuery<T extends object>(
    ctor: new () => T,
    opts: {
      includeTree?: IncludeTree<T> | null;
      from?: string;
      tagCte?: string;
    }
  ): Either<string, string> {
    const { wasm } = RuntimeContainer.get();
    const args = [
      WasmResource.fromString(ctor.name, wasm),
      WasmResource.fromString(JSON.stringify(opts.includeTree ?? null), wasm),
      WasmResource.fromString(JSON.stringify(opts.tagCte ?? null), wasm),
      WasmResource.fromString(JSON.stringify(opts.from ?? null), wasm),
    ];
    return invokeOrmWasm(wasm.list_models, args, wasm);
  }

  /**
   * Returns a query to get a single model by primary key, using the provided include tree.
   * Optionally, a `from` string can be provided to customize the source of the query.
   * The `from` string should be a valid `SELECT` statement returning all columns from the
   * desired source.
   *
   * Example:
   * ```ts
   * const query = Orm.getQuery(Person, Person.default);
   * ```
   */
  static getQuery<T extends object>(
    ctor: new () => T,
    includeTree?: IncludeTree<T> | null
  ): Either<string, string> {
    const { ast } = RuntimeContainer.get();
    return this.listQuery<T>(ctor, {
      includeTree,
    }).map(
      (inner) =>
        `${inner} WHERE [${ast.models[ctor.name].primary_key.name}] = ?`
    );
  }

  /**
   * Retrieves all instances of a model from the database.
   * @param ctor The model constructor.
   * @param includeTree An include tree describing which related models to join.
   * @param from An optional custom `FROM` clause to use instead of the base table.
   * @returns Either an error string, or an array of model instances.
   *
   * Example:
   * ```ts
   * const orm = Orm.fromD1(env.db);
   * const horses = await orm.list(Horse, Horse.default);
   * ```
   *
   * will translate to the SQL query:
   * ```sql
   * SELECT
   *  "Horse"."id" AS "id",
   *  ...
   * FROM "Horse"
   * LEFT JOIN ...
   * ```
   *
   *
   * Example with custom from:
   * ```ts
   * const orm = Orm.fromD1(env.db);
   * const adultHorses = await orm.list(Horse, Horse.default, "SELECT * FROM Horse ORDER BY age DESC LIMIT 10");
   * ```
   *
   * will translate to the SQL query:
   * ```sql
   * SELECT
   *  "Horse"."id" AS "id",
   * ...
   * FROM (SELECT * FROM Horse ORDER BY age DESC LIMIT 10)
   * LEFT JOIN ...
   *
   */
  async list<T extends object>(
    ctor: new () => T,
    opts: {
      includeTree?: IncludeTree<T> | null;
      from?: string;
    }
  ): Promise<Either<string, T[]>> {
    const queryRes = Orm.listQuery(ctor, opts);
    if (queryRes.isLeft()) {
      return Either.left(queryRes.value);
    }

    const stmt = this.db.prepare(queryRes.value);
    const records = await stmt.all();
    if (!records.success) {
      return Either.left(
        records.error ?? "D1 query failed, but no error was returned."
      );
    }

    const mapRes = Orm.mapSql(ctor, records.results, opts.includeTree ?? null);
    if (mapRes.isLeft()) {
      return Either.left(mapRes.value);
    }

    return Either.right(mapRes.value as T[]);
  }

  /**
   * Retrieves a single model by primary key.
   * @param ctor The model constructor.
   * @param id The primary key value.
   * @param includeTree An include tree describing which related models to join.
   * @returns Either an error string, or the model instance (null if not found).
   *
   * Example:
   * ```ts
   * const orm = Orm.fromD1(env.db);
   * const horse = await orm.get(Horse, 1, Horse.default);
   * ```
   */
  async get<T extends object>(
    ctor: new () => T,
    id: any,
    includeTree?: IncludeTree<T> | null
  ): Promise<Either<string, T | null>> {
    const queryRes = Orm.getQuery(ctor, includeTree);
    if (queryRes.isLeft()) {
      return Either.left(queryRes.value);
    }

    const record = await this.db.prepare(queryRes.value).bind(id).run();

    if (!record.success) {
      return Either.left(
        record.error ?? "D1 query failed, but no error was returned."
      );
    }

    if (record.results.length === 0) {
      return Either.right(null);
    }

    const mapRes = Orm.mapSql(ctor, record.results, includeTree);
    if (mapRes.isLeft()) {
      return Either.left(mapRes.value);
    }

    return Either.right(mapRes.value[0] as T);
  }
}
