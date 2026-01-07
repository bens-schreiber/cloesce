import type {
  ReadableStream,
  R2Bucket,
  D1Database,
  KVNamespace,
  D1Result,
} from "@cloudflare/workers-types";

import { DeepPartial, KValue, KeysOfType, u8ToB64 } from "./common.js";
import { RuntimeContainer } from "../router/router.js";
import { WasmResource, mapSqlJson, invokeOrmWasm } from "../router/wasm.js";
import { CrudKind, Model as AstModel } from "../ast.js";
import Either from "../either.js";

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
  (_uniqueId: string): PropertyDecorator =>
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

export class Orm {
  private constructor(private env: unknown) {}

  /**
   * Creates an instance of an `Orm`
   * @param env The Wrangler environment containing Cloudflare bindings.
   */
  static fromEnv(env: unknown): Orm {
    // TODO: We could validate that `env` is of the correct type defined by the `＠WranglerEnv` class
    // by putting the class definition in the Constructor Registry at compile time.
    return new Orm(env);
  }

  // TODO: support multiple D1 bindings
  private get db(): D1Database {
    const { ast } = RuntimeContainer.get();
    return (this.env as any)[ast.wrangler_env!.d1_binding!];
  }

  /**
   * Hydrates a base object into an instantiated object, including
   * navigation properties, KV objects, and R2 objects as defined
   * in the model AST.
   *
   * @param ctor The model constructor
   * @param base The base object to hydrate
   * @param keyParams Key parameters to assign during hydration
   * @param includeTree Include tree to define the relationships to hydrate.
   * @returns The hydrated model instance
   */
  async hydrate<T extends object>(
    ctor: new () => T,
    base: any,
    keyParams: Record<string, string>,
    includeTree: IncludeTree<T> | null = null,
  ): Promise<T> {
    const { ast, constructorRegistry } = RuntimeContainer.get();
    const model = ast.models[ctor.name];

    if (!model) {
      throw new Error(`Model ${ctor.name} not found in AST`);
    }

    const instance: T = Object.assign(
      new constructorRegistry[model.name](),
      base,
    );
    const promises: Promise<void>[] = [];
    const env: any = this.env;

    recurse(instance, model, includeTree ?? {});
    await Promise.all(promises);

    return instance;

    function resolveKey(format: string, current: any): string {
      return format.replace(/\{([^}]+)\}/g, (_, paramName) => {
        const paramValue = keyParams[paramName] ?? current[paramName];
        if (!paramValue) {
          throw new Error(
            `Parameter ${paramName} was missing during hydration`,
          );
        }
        return String(paramValue);
      });
    }

    async function hydrateKVList(
      namespace: KVNamespace,
      key: string,
      kv: any,
      current: any,
    ) {
      const res = await namespace.list({ prefix: key });

      if (kv.value.cidl_type === "Stream") {
        current[kv.value.name] = await Promise.all(
          res.keys.map(async (k: any) => {
            const stream = await namespace.get(k.name, { type: "stream" });
            return {
              key: k.name,
              value: stream,
              raw: stream,
              metadata: null,
            } satisfies KValue<ReadableStream>;
          }),
        );
      } else {
        current[kv.value.name] = await Promise.all(
          res.keys.map(async (k: any) => {
            const kvRes = await namespace.getWithMetadata(k.name, {
              type: "json",
            });
            return {
              key: k.name,
              value: kvRes.value,
              raw: kvRes.value,
              metadata: kvRes.metadata,
            } satisfies KValue<unknown>;
          }),
        );
      }
    }

    async function hydrateKVSingle(
      namespace: KVNamespace,
      key: string,
      kv: any,
      current: any,
    ) {
      if (kv.value.cidl_type === "Stream") {
        const res = await namespace.get(key, { type: "stream" });
        current[kv.value.name] = {
          key,
          value: res,
          raw: res,
          metadata: null,
        } satisfies KValue<ReadableStream>;
      } else {
        const res = await namespace.getWithMetadata(key, { type: "json" });
        current[kv.value.name] = {
          key,
          value: res.value,
          raw: res.value,
          metadata: res.metadata,
        } satisfies KValue<unknown>;
      }
    }

    function recurse(current: any, meta: AstModel, tree: IncludeTree<any>) {
      // Hydrate navigation properties
      for (const navProp of meta.navigation_properties) {
        const nestedTree = tree[navProp.var_name];
        if (!nestedTree) continue;

        const nestedMeta = ast.models[navProp.model_reference];
        const value = current[navProp.var_name];

        if (Array.isArray(value)) {
          current[navProp.var_name] = value.map((child) => {
            const instance = Object.assign(
              new constructorRegistry[nestedMeta.name](),
              child,
            );
            recurse(instance, nestedMeta, nestedTree);
            return instance;
          });
        } else if (value) {
          current[navProp.var_name] = Object.assign(
            new constructorRegistry[nestedMeta.name](),
            value,
          );
          recurse(current[navProp.var_name], nestedMeta, nestedTree);
        }
      }

      // Hydrate columns
      for (const col of meta.columns) {
        switch (col.value.cidl_type) {
          case "DateIso": {
            current[col.value.name] = new Date(current[col.value.name]);
            break;
          }
          case "Blob": {
            const arr: number[] = current[col.value.name];
            current[col.value.name] = new Uint8Array(arr);
          }
          default: {
            break;
          }
        }
      }

      // Assign key params
      for (const keyParam of meta.key_params) {
        current[keyParam] = keyParams[keyParam];
      }

      // Hydrate KV objects
      for (const kv of meta.kv_objects) {
        if (tree[kv.value.name] === undefined) {
          if (kv.list_prefix) {
            current[kv.value.name] = [];
          }

          continue;
        }

        const key = resolveKey(kv.format, current);
        const namespace: KVNamespace = env[kv.namespace_binding];

        if (kv.list_prefix) {
          promises.push(hydrateKVList(namespace, key, kv, current));
        } else {
          promises.push(hydrateKVSingle(namespace, key, kv, current));
        }
      }

      // Hydrate R2 objects
      for (const r2 of meta.r2_objects) {
        if (tree[r2.var_name] === undefined) {
          if (r2.list_prefix) {
            current[r2.var_name] = [];
          }

          continue;
        }

        const key = resolveKey(r2.format, current);
        const bucket: R2Bucket = env[r2.bucket_binding];

        if (r2.list_prefix) {
          promises.push(
            (async () => {
              const list = await bucket.list({ prefix: key });

              current[r2.var_name] = await Promise.all(
                list.objects.map(async (obj) => {
                  const fullObj = await bucket.get(obj.key);
                  return fullObj;
                }),
              );
            })(),
          );
        } else {
          promises.push(
            (async () => {
              const obj = await bucket.get(key);
              current[r2.var_name] = obj;
            })(),
          );
        }
      }
    }
  }

  /**
   * Maps SQL records to an instantiated Model. The records must be flat
   * (e.g., of the form "id, name, address") or derive from a Cloesce data source view
   * (e.g., of the form "Horse.id, Horse.name, Horse.address")
   *
   * Assumes the data is formatted correctly, throwing an error otherwise.
   *
   * @param ctor The model constructor
   * @param records D1 Result records
   * @param includeTree Include tree to define the relationships to join.
   */
  async mapSql<T extends object>(
    ctor: new () => T,
    records: Record<string, any>[],
    includeTree: IncludeTree<T> | null = null,
  ): Promise<T[]> {
    const base = mapSqlJson(ctor, records, includeTree).unwrap();
    for (const key in base) {
      base[key] = await this.hydrate(ctor, base[key], {}, includeTree);
    }

    return base;
  }

  /**
   * Executes an "upsert" query, adding or augmenting a model in the database.
   *
   * If a model's primary key is not defined in `newModel`, the query is assumed to be an insert.
   *
   * If a model's primary key _is_ defined, but some attributes are missing, the query is assumed to be an update.
   *
   * Finally, if the primary key is defined, but all attributes are included, a SQLite upsert will be performed.
   *
   * In any other case, an  error will be thrown.
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
   * @returns The primary key of the inserted model.
   */
  async upsert<T extends object>(
    ctor: new () => T,
    newModel: DeepPartial<T>,
    includeTree: IncludeTree<T> | null = null,
  ): Promise<Either<D1Result, any>> {
    const { wasm } = RuntimeContainer.get();
    const args = [
      WasmResource.fromString(ctor.name, wasm),
      WasmResource.fromString(
        JSON.stringify(newModel, (k, v) =>
          v instanceof Uint8Array ? u8ToB64(v) : v,
        ),
        wasm,
      ),
      WasmResource.fromString(JSON.stringify(includeTree), wasm),
    ];

    const upsertQueryRes = invokeOrmWasm(wasm.upsert_model, args, wasm);
    if (upsertQueryRes.isLeft()) {
      throw new Error(`Upsert failed internally: ${upsertQueryRes.value}`);
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
      statements.map((s) => this.db.prepare(s.query).bind(...s.values)),
    );

    const failed = batchRes.find((r) => !r.success);
    if (failed) {
      // throw new Error("Upsert failed: " + (failed.error ?? "unknown error"));
      return Either.left(failed);
    }

    const rootModelId = batchRes[selectIndex!].results[0] as { id: any };
    return Either.right(rootModelId.id);
  }

  /**
   * Returns a select query, creating a CTE view for the model using the provided include tree.
   *
   * @param ctor The model constructor.
   * @param includeTree An include tree describing which related models to join.
   * @param from An optional custom `FROM` clause to use instead of the base table.
   * @param tagCte An optional CTE name to tag the query with. Defaults to "Model.view".
   *
   * ### Example:
   * ```ts
   * // Using a data source
   * const query = Orm.listQuery(Person, "default");
   *
   * // Using a custom from statement
   * const query = Orm.listQuery(Person, null, "SELECT * FROM Person WHERE age > 18");
   * ```
   *
   * ### Example SQL output:
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
    },
  ): string {
    const { wasm } = RuntimeContainer.get();
    const args = [
      WasmResource.fromString(ctor.name, wasm),
      WasmResource.fromString(JSON.stringify(opts.includeTree ?? null), wasm),
      WasmResource.fromString(JSON.stringify(opts.tagCte ?? null), wasm),
      WasmResource.fromString(JSON.stringify(opts.from ?? null), wasm),
    ];

    const res = invokeOrmWasm(wasm.list_models, args, wasm);
    if (res.isLeft()) {
      throw new Error(`Error invoking the Cloesce WASM Binary: ${res.value}`);
    }

    return res.unwrap();
  }

  /**
   * Returns a select query for a single model by primary key, creating a CTE view using the provided include tree.
   *
   * @param ctor The model constructor.
   * @param includeTree An include tree describing which related models to join.
   *
   * ### Example:
   * ```ts
   * // Using a data source
   * const query = Orm.getQuery(Person, "default");
   * ```
   *
   * ### Example SQL output:
   *
   * ```sql
   * WITH Person_view AS (
   * SELECT
   * "Person"."id" AS "id",
   * ...
   * FROM "Person"
   * LEFT JOIN ...
   * )
   * SELECT * FROM Person_view WHERE [Person].[id] = ?
   * ```
   */
  static getQuery<T extends object>(
    ctor: new () => T,
    includeTree?: IncludeTree<T> | null,
  ): string {
    const { ast } = RuntimeContainer.get();
    // TODO: handle missing primary key
    return `${this.listQuery<T>(ctor, { includeTree })} WHERE [${ast.models[ctor.name].primary_key!.name}] = ?`;
  }

  /**
   * Retrieves all instances of a model from the database.
   * @param ctor The model constructor.
   * @param includeTree An include tree describing which related models to join.
   * @param from An optional custom `FROM` clause to use instead of the base table.
   * @returns Either an error string, or an array of model instances.
   *
   * ### Example:
   * ```ts
   * const orm = Orm.fromD1(env.db);
   * const horses = await orm.list(Horse, Horse.default);
   * ```
   *
   * ### Example with custom from:
   * ```ts
   * const orm = Orm.fromD1(env.db);
   * const adultHorses = await orm.list(Horse, Horse.default, "SELECT * FROM Horse ORDER BY age DESC LIMIT 10");
   * ```
   *
   * =>
   *
   * ```sql
   * SELECT
   *  "Horse"."id" AS "id",
   * ...
   * FROM (SELECT * FROM Horse ORDER BY age DESC LIMIT 10)
   * LEFT JOIN ...
   * ```
   *
   */
  async list<T extends object>(
    ctor: new () => T,
    opts: {
      includeTree?: IncludeTree<T> | null;
      from?: string;
    },
  ): Promise<Either<D1Result, T[]>> {
    const sql = Orm.listQuery(ctor, opts);

    const stmt = this.db.prepare(sql);
    const records = await stmt.all();
    if (!records.success) {
      return Either.left(records);
    }

    const mapped = await this.mapSql(
      ctor,
      records.results,
      opts.includeTree ?? null,
    );
    return Either.right(mapped);
  }

  /**
   * Retrieves a single model by primary key.
   * @param ctor The model constructor.
   * @param id The primary key value.
   * @param includeTree An include tree describing which related models to join.
   * @returns Either an error string, or the model instance (null if not found).
   *
   * ### Example:
   * ```ts
   * const orm = Orm.fromD1(env.db);
   * const horse = await orm.get(Horse, 1, Horse.default);
   * ```
   */
  async get<T extends object>(
    ctor: new () => T,
    id: any,
    includeTree?: IncludeTree<T> | null,
  ): Promise<Either<D1Result, T | null>> {
    const sql = Orm.getQuery(ctor, includeTree);
    const record = await this.db.prepare(sql).bind(id).run();

    if (!record.success) {
      throw new Error("Get query failed: " + (record.error ?? "unknown error"));
    }

    if (record.results.length === 0) {
      return Either.right(null);
    }

    const mapped = await this.mapSql(ctor, record.results, includeTree);
    return Either.right(mapped[0]);
  }
}
